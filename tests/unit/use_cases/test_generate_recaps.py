from dataclasses import dataclass
from datetime import UTC, datetime, timedelta

import httpx
import pytest
from sqlalchemy import select

from src.config.configs import AppConfigs
from src.domain.recaps import RecapSettings
from src.infrastructure.ai.pi_client import PiClient, PiError
from src.infrastructure.database.models import SentRecapStory
from src.infrastructure.database.orm import Database
from src.infrastructure.telegram.bot_api import TelegramBotApi
from src.infrastructure.telegram.errors import TelegramApiError
from src.use_cases.generate_recaps import AUTH_ALERT_TEXT, NewsRecapService
from src.use_cases.manage_settings import SettingsService

pytestmark = pytest.mark.unit

NOW = datetime(2026, 10, 20, 18, 0, tzinfo=UTC)
INTERVAL = timedelta(hours=6)


def build_output(*urls: str) -> str:
    """Create a Pi recap reply with one story per URL."""
    stories = ", ".join(
        f'{{"emoji": "📰", "title": "Story {index}", "summary": "Summary.", "source_name": "Source", "url": "{url}"}}'
        for index, url in enumerate(urls)
    )
    return f'{{"stories": [{stories}]}}'


class FakePiClient(PiClient):
    def __init__(self, output: str | Exception, *, authenticated: bool = True) -> None:
        """
        Create a fake Pi client with a fixed reply and sign-in state.
        """
        self.output = output
        self.authenticated = authenticated
        self.prompts: list[tuple[str, str, str]] = []

    async def run_prompt(self, prompt: str, model: str, thinking: str) -> str:
        """
        Record the prompt and return the configured reply or raise the configured failure.
        """
        self.prompts.append((prompt, model, thinking))
        if isinstance(self.output, Exception):
            raise self.output
        return self.output

    async def list_models(self) -> list[str]:
        """
        Return a fixed model catalog.
        """
        return ["gpt-6-luna", "gpt-6-sol"]

    async def is_authenticated(self) -> bool:
        """
        Return the configured sign-in state.
        """
        return self.authenticated


class FakeTelegramApi(TelegramBotApi):
    def __init__(self, error: Exception | None = None) -> None:
        """
        Create a fake Telegram API that records messages or fails HTML sends.
        """
        self.error = error
        self.html_messages: list[tuple[int | str, str, bool]] = []
        self.text_messages: list[tuple[int | str, str]] = []

    async def send_html_message(self, chat_id: int | str, text: str, *, disable_web_page_preview: bool = False) -> None:
        """
        Record a sent HTML message or raise the configured failure.
        """
        if self.error is not None:
            raise self.error
        self.html_messages.append((chat_id, text, disable_web_page_preview))

    async def send_text_message(self, chat_id: int | str, text: str) -> None:
        """
        Record a sent plain text message.
        """
        self.text_messages.append((chat_id, text))


@dataclass(frozen=True)
class RecapHarness:
    db: Database
    settings: SettingsService
    service: NewsRecapService


@pytest.fixture
def recap_harness(app_configs: AppConfigs, database: Database, settings_service: SettingsService) -> RecapHarness:
    """
    Create a recap service with recaps on, due now, and one previously sent story.
    """
    with database.create_session() as session:
        settings_service.set_destination_chat_id(session, "@dest")
        settings_service.set_recap_settings(
            session,
            RecapSettings(
                topics=["AI"],
                language="English",
                interval_seconds=int(INTERVAL.total_seconds()),
                enabled=True,
                model="gpt-6-luna",
                thinking="high",
            ),
        )
        settings_service.set_recap_next_run_at(session, NOW)
        session.add(SentRecapStory(url="https://news.example/old", title="Old", sent_at=NOW - timedelta(days=1)))
        session.add(SentRecapStory(url="https://news.example/stale", title="Stale", sent_at=NOW - timedelta(days=60)))
        session.commit()
    return RecapHarness(database, settings_service, NewsRecapService(app_configs, database, settings_service))


def use_fakes(
    harness: RecapHarness, pi: FakePiClient, telegram: FakeTelegramApi | None = None
) -> tuple[FakePiClient, FakeTelegramApi]:
    """
    Replace the recap service's Pi and Telegram boundaries with fakes.
    """
    telegram = telegram or FakeTelegramApi()
    harness.service.pi = pi
    harness.service.telegram = telegram
    return pi, telegram


def read_next_run_at(harness: RecapHarness) -> datetime | None:
    """
    Read the stored next recap time.
    """
    with harness.db.create_session() as session:
        return harness.settings.get_recap_next_run_at(session)


def read_sent_urls(harness: RecapHarness) -> list[str]:
    """
    Read remembered story URLs ordered by insertion.
    """
    with harness.db.create_session() as session:
        return list(session.scalars(select(SentRecapStory.url).order_by(SentRecapStory.id)))


@pytest.mark.asyncio
async def test_due_recap_is_sent_once_without_repeating_stories(recap_harness: RecapHarness) -> None:
    """Send new stories without previews, remember them, prune old history, and schedule the next recap."""
    pi, telegram = use_fakes(
        recap_harness, FakePiClient(build_output("https://news.example/old", "https://news.example/new"))
    )

    await recap_harness.service.run_if_due(NOW)
    await recap_harness.service.run_if_due(NOW)

    [(prompt, model, thinking)] = pi.prompts
    assert (model, thinking) == ("gpt-6-luna", "high")
    assert "since 2026-10-19 18:00 UTC" in prompt
    assert "- Old (https://news.example/old)" in prompt
    [(chat_id, message, disable_preview)] = telegram.html_messages
    assert (chat_id, disable_preview) == ("@dest", True)
    assert "https://news.example/new" in message
    assert "https://news.example/old" not in message
    assert read_sent_urls(recap_harness) == ["https://news.example/old", "https://news.example/new"]
    assert read_next_run_at(recap_harness) == NOW + INTERVAL


@pytest.mark.asyncio
async def test_failed_generation_retries_soon_while_pi_is_signed_in(
    recap_harness: RecapHarness, app_configs: AppConfigs
) -> None:
    """Retry after the retry delay when Pi fails or replies with an unusable recap."""
    for output in (PiError("rate limited"), "not json"):
        _, telegram = use_fakes(recap_harness, FakePiClient(output))
        with recap_harness.db.create_session() as session:
            recap_harness.settings.set_recap_next_run_at(session, NOW)
            session.commit()

        await recap_harness.service.run_if_due(NOW)

        assert telegram.html_messages == []
        assert telegram.text_messages == []
        assert read_next_run_at(recap_harness) == NOW + timedelta(seconds=app_configs.recap_retry_delay_seconds)


@pytest.mark.asyncio
async def test_lost_sign_in_alerts_once_per_interval(recap_harness: RecapHarness) -> None:
    """Tell the user to sign in again and wait a full interval instead of retrying soon."""
    _, telegram = use_fakes(recap_harness, FakePiClient(PiError("unauthorized"), authenticated=False))

    await recap_harness.service.run_if_due(NOW)

    assert telegram.text_messages == [("@dest", AUTH_ALERT_TEXT)]
    assert read_next_run_at(recap_harness) == NOW + INTERVAL


@pytest.mark.asyncio
@pytest.mark.parametrize(
    ("error", "delay"),
    [
        (TelegramApiError("sendMessage", "Too Many Requests", 429), timedelta(minutes=15)),
        (httpx.ConnectError("offline"), timedelta(minutes=15)),
        (TelegramApiError("sendMessage", "Bad Request", 400), INTERVAL),
    ],
)
async def test_delivery_failures_reschedule_without_remembering_stories(
    recap_harness: RecapHarness, error: Exception, delay: timedelta
) -> None:
    """Retry transient Telegram failures soon, and skip to the next interval after a rejection."""
    use_fakes(recap_harness, FakePiClient(build_output("https://news.example/new")), FakeTelegramApi(error))

    await recap_harness.service.run_if_due(NOW)

    assert read_next_run_at(recap_harness) == NOW + delay
    assert "https://news.example/new" not in read_sent_urls(recap_harness)


@pytest.mark.asyncio
async def test_recap_waits_until_due_and_publishes_models(recap_harness: RecapHarness) -> None:
    """Skip early checks, and store the account's models for the Mini App once per refresh interval."""
    pi, _ = use_fakes(recap_harness, FakePiClient(build_output()))

    await recap_harness.service.run_if_due(NOW - timedelta(minutes=1))
    await recap_harness.service.refresh_models_if_stale(NOW)

    assert pi.prompts == []
    with recap_harness.db.create_session() as session:
        assert recap_harness.settings.get_recap_available_models(session) == ["gpt-6-luna", "gpt-6-sol"]
