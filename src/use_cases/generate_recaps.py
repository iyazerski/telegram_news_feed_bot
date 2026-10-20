import asyncio
import json
from dataclasses import dataclass
from datetime import datetime, timedelta

import httpx
from loguru import logger
from sqlalchemy import delete, select

from src.config.configs import AppConfigs
from src.domain.recaps import RecapSettings, RecapStory, select_new_stories
from src.infrastructure.ai.news_recap_prompt import build_news_recap_prompt, parse_news_recap_output
from src.infrastructure.ai.pi_client import PiClient, PiError
from src.infrastructure.database.models import SentRecapStory
from src.infrastructure.database.orm import Database
from src.infrastructure.telegram.bot_api import TelegramBotApi
from src.infrastructure.telegram.errors import TelegramApiError
from src.infrastructure.telegram.recap_formatter import NewsRecapFormatter
from src.use_cases.manage_settings import SettingsService

# Short intervals would only see a few hours of news, so research always covers at least a day.
# Story history keeps longer windows from repeating what earlier recaps sent.
MIN_RECAP_WINDOW = timedelta(hours=24)
AUTH_ALERT_TEXT = (
    "⚠️ AI news recaps are paused because Pi is not signed in to ChatGPT. "
    "Sign in again and copy the new auth.json to the recapper volume."
)


@dataclass(frozen=True)
class DueRecap:
    settings: RecapSettings
    destination: str
    # Newest first, as (title, url) pairs.
    sent_stories: list[tuple[str, str]]


class NewsRecapService:
    def __init__(self, configs: AppConfigs, db: Database, settings: SettingsService) -> None:
        """
        Create the use case that researches, formats, and sends AI news recaps.
        """
        self.configs = configs
        self.db = db
        self.settings = settings
        self.pi = PiClient(configs)
        self.formatter = NewsRecapFormatter()
        self.telegram = TelegramBotApi(configs.bot_token, configs.telegram_http_timeout_seconds)
        self.models_refreshed_at: datetime | None = None

    async def close(self) -> None:
        """
        Close persistent HTTP clients owned by the recap service.
        """
        await self.telegram.close()

    async def refresh_models_if_stale(self, now: datetime) -> None:
        """
        Publish the signed-in account's model IDs for the Mini App when the stored list is stale.
        """
        refresh_interval = timedelta(seconds=self.configs.recap_model_refresh_interval_seconds)
        if self.models_refreshed_at is not None and now - self.models_refreshed_at < refresh_interval:
            return

        self.models_refreshed_at = now
        try:
            models = await self.pi.list_models()
        except (PiError, TimeoutError, OSError, json.JSONDecodeError) as exc:
            logger.warning(f"Could not list Pi models: {exc}")
            return
        await asyncio.to_thread(self.save_available_models, models)

    async def run_if_due(self, now: datetime) -> None:
        """
        Generate and send one recap when the schedule says it is due.
        """
        due = await asyncio.to_thread(self.load_due_recap, now)
        if due is None:
            return

        settings = due.settings
        prompt = build_news_recap_prompt(
            settings,
            now,
            now - max(timedelta(seconds=settings.interval_seconds), MIN_RECAP_WINDOW),
            self.configs.recap_story_count,
            due.sent_stories,
        )
        try:
            output = await self.pi.run_prompt(prompt, settings.model, settings.thinking)
            stories = parse_news_recap_output(output)
        except (PiError, TimeoutError, ValueError) as exc:
            await self.handle_generation_failure(due, now, exc)
            return

        stories = select_new_stories(stories, [url for _, url in due.sent_stories], self.configs.recap_story_count)
        message = self.formatter.build_message(stories, settings.topics)
        try:
            await self.telegram.send_html_message(due.destination, message, disable_web_page_preview=True)
        except TelegramApiError as exc:
            if exc.is_retryable:
                logger.warning(f"Telegram is unavailable for the recap; retrying later: {exc.description}")
                await asyncio.to_thread(self.schedule, now + self.retry_delay())
                return
            # A rejected message would be rejected again, so wait for the next regular recap.
            logger.error(f"Telegram rejected the recap: {exc.description}")
            await asyncio.to_thread(self.schedule, now + timedelta(seconds=settings.interval_seconds))
            return
        except httpx.HTTPError as exc:
            logger.warning(f"Recap delivery failed; retrying later: {exc}")
            await asyncio.to_thread(self.schedule, now + self.retry_delay())
            return

        await asyncio.to_thread(self.commit_recap, stories, now, settings.interval_seconds)
        logger.info(f"Sent a recap with {len(stories)} stories")

    async def handle_generation_failure(self, due: DueRecap, now: datetime, error: Exception) -> None:
        """
        Retry soon after transient failures, and alert once per interval when Pi lost its sign-in.
        """
        if await self.pi.is_authenticated():
            logger.warning(f"Recap generation failed; retrying later: {error}")
            await asyncio.to_thread(self.schedule, now + self.retry_delay())
            return

        logger.error(f"Recap generation failed because Pi is not signed in: {error}")
        try:
            await self.telegram.send_text_message(due.destination, AUTH_ALERT_TEXT)
        except TelegramApiError, httpx.HTTPError:
            logger.exception("Could not send the Pi sign-in alert")
        await asyncio.to_thread(self.schedule, now + timedelta(seconds=due.settings.interval_seconds))

    def retry_delay(self) -> timedelta:
        """
        Return the delay before retrying a failed recap.
        """
        return timedelta(seconds=self.configs.recap_retry_delay_seconds)

    def load_due_recap(self, now: datetime) -> DueRecap | None:
        """
        Read the recap inputs when recaps are on, due, and have a destination chat.
        """
        with self.db.create_session() as session:
            settings = self.settings.get_recap_settings(session, self.configs.default_recap_interval_seconds)
            next_run_at = self.settings.get_recap_next_run_at(session)
            if not settings.enabled or not settings.topics or next_run_at is None or next_run_at > now:
                return None

            destination = self.settings.get_destination_chat_id(session)
            if destination is None:
                return None

            rows = session.execute(
                select(SentRecapStory.title, SentRecapStory.url)
                .order_by(SentRecapStory.sent_at.desc())
                .limit(self.configs.recap_history_size)
            )
            return DueRecap(settings, destination, [(title, url) for title, url in rows])

    def commit_recap(self, stories: list[RecapStory], now: datetime, interval_seconds: int) -> None:
        """
        Remember sent stories, prune old history, and schedule the next recap.
        """
        with self.db.create_session() as session:
            session.add_all(SentRecapStory(url=story.url, title=story.title, sent_at=now) for story in stories)
            cutoff = now - timedelta(days=self.configs.recap_history_retention_days)
            session.execute(delete(SentRecapStory).where(SentRecapStory.sent_at < cutoff))
            self.settings.set_recap_next_run_at(session, now + timedelta(seconds=interval_seconds))
            session.commit()

    def schedule(self, run_at: datetime) -> None:
        """
        Store when the next recap attempt runs.
        """
        with self.db.create_session() as session:
            self.settings.set_recap_next_run_at(session, run_at)
            session.commit()

    def save_available_models(self, models: list[str]) -> None:
        """
        Store the signed-in account's model IDs for the Mini App.
        """
        with self.db.create_session() as session:
            self.settings.set_recap_available_models(session, models)
            session.commit()
