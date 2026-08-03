import httpx

from src.config.configs import AppConfigs
from src.domain.delivery import DeliveryResult
from src.infrastructure.database.models import SourceChannel
from src.infrastructure.database.orm import Database
from src.infrastructure.messaging.events import PostReferenceEvent
from src.infrastructure.telegram.bot_api import TelegramBotApi
from src.infrastructure.telegram.errors import TelegramApiError
from src.infrastructure.telegram.media_downloader import TelegramMediaDownloader, UnsupportedPreviewMediaError
from src.infrastructure.telegram.message_formatter import TelegramMessageFormatter
from src.use_cases.manage_channels import ChannelService
from src.use_cases.manage_settings import SettingsService


class TelegramForwardingService:
    def __init__(self, configs: AppConfigs, db: Database, settings: SettingsService) -> None:
        """
        Create the use case that reposts discovered Telegram posts.
        """
        self.db = db
        self.settings = settings
        self.channels = ChannelService()
        self.formatter = TelegramMessageFormatter()
        self.media_downloader = TelegramMediaDownloader(
            configs.telegram_http_timeout_seconds,
            configs.dispatcher_media_max_bytes,
        )
        self.telegram = TelegramBotApi(configs.bot_token, configs.telegram_http_timeout_seconds)

    async def forward_event(self, event: PostReferenceEvent) -> DeliveryResult:
        """
        Repost one discovered source post as one Telegram bot delivery.
        """
        with self.db.create_session() as session:
            channel = self.channels.get_active_channel(session, event.source_channel)
            if event.message_id <= channel.last_committed_message_id:
                return DeliveryResult(action="ack")
            destination = self.settings.get_destination_chat_id(session)

        if destination is None:
            return DeliveryResult(action="retry")

        try:
            await self.deliver_post(destination, event)
        except TelegramApiError as exc:
            if self.is_retryable_telegram_error(exc):
                return DeliveryResult(action="retry")

            self.commit_event(event)
            return DeliveryResult(action="term", error=exc.description)
        except httpx.HTTPError:
            return DeliveryResult(action="retry")

        self.commit_event(event)
        return DeliveryResult(action="ack")

    async def deliver_post(self, destination: str, event: PostReferenceEvent) -> None:
        """
        Deliver a discovered source post as one message or grouped album.
        """
        caption_html = self.formatter.build_media_caption(event)
        try:
            media = await self.media_downloader.download_media(event.media_urls)
        except UnsupportedPreviewMediaError:
            await self.send_text_messages(destination, event)
            return

        if len(media) == 1:
            if media[0].content_type.startswith("video/"):
                await self.telegram.send_video(destination, media[0], caption_html)
            else:
                await self.telegram.send_photo(destination, media[0], caption_html)
            await self.send_body_messages(destination, event, caption_html)
            return

        if len(media) > 1:
            await self.telegram.send_media_group(destination, media, caption_html)
            await self.send_body_messages(destination, event, caption_html)
            return

        await self.send_text_messages(destination, event)

    async def send_text_messages(self, destination: str, event: PostReferenceEvent) -> None:
        """
        Send all text chunks required to deliver a source post without truncation.
        """
        for message in self.formatter.build_text_messages(event):
            await self.telegram.send_html_message(destination, message)

    async def send_body_messages(self, destination: str, event: PostReferenceEvent, caption_html: str) -> None:
        """
        Send the body separately when the media caption could not contain the full source text.
        """
        if caption_html == self.formatter.build_message_text(event):
            return

        for message in self.formatter.build_body_messages(event):
            await self.telegram.send_html_message(destination, message)

    def is_retryable_telegram_error(self, error: TelegramApiError) -> bool:
        """
        Return whether Telegram rejected delivery for a transient reason.
        """
        return error.error_code == 429 or error.error_code >= 500

    def commit_event(self, event: PostReferenceEvent) -> None:
        """
        Advance the source cursor after a post reference has been processed by the dispatcher.
        """
        with self.db.create_session() as session:
            channel = self.channels.get_active_channel(session, event.source_channel)
            self.commit_channel_message(channel, event.message_id)
            session.commit()

    def commit_channel_message(self, channel: SourceChannel, message_id: int) -> None:
        """
        Advance a channel cursor to the processed message ID.
        """
        channel.last_committed_message_id = max(channel.last_committed_message_id, message_id)
