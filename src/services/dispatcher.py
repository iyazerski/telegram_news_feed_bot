import html

import httpx
from bs4 import BeautifulSoup

from src.configs.configs import AppConfigs
from src.models.database import SourceChannel
from src.models.delivery import DeliveryResult
from src.models.events import PostReferenceEvent
from src.models.media import TelegramUpload
from src.orm.database import Database
from src.services.channels import ChannelService
from src.services.settings import SettingsService
from src.services.telegram_api import TelegramApiError, TelegramBotApi

PHOTO_CAPTION_LIMIT = 1024
TEXT_MESSAGE_LIMIT = 4096
MEDIA_GROUP_LIMIT = 10
SUPPORTED_MEDIA_CONTENT_TYPES = {"image/jpeg": "jpg", "image/png": "png"}


class UnsupportedPreviewMediaError(Exception):
    """Represent preview media that cannot be uploaded as a Telegram photo."""


class TelegramForwardingService:
    def __init__(self, configs: AppConfigs, db: Database, settings: SettingsService) -> None:
        """
        Create the internal service that reposts discovered Telegram posts.
        """
        self.db = db
        self.settings = settings
        self.channels = ChannelService()
        self.media_timeout_seconds = configs.telegram_http_timeout_seconds
        self.media_max_bytes = configs.dispatcher_media_max_bytes
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
        caption_html = self.build_media_caption(event)
        try:
            media = await self.download_media(event.media_urls)
        except UnsupportedPreviewMediaError:
            await self.telegram.send_html_message(destination, self.build_text_message(event))
            return

        if len(media) == 1:
            await self.telegram.send_photo(destination, media[0], caption_html)
            return

        if len(media) > 1:
            await self.telegram.send_media_group(destination, media, caption_html)
            return

        await self.telegram.send_html_message(destination, self.build_text_message(event))

    async def download_media(self, media_urls: list[str]) -> list[TelegramUpload]:
        """
        Download preview media and prepare multipart upload files for Telegram delivery.
        """
        media: list[TelegramUpload] = []
        async with httpx.AsyncClient(timeout=self.media_timeout_seconds) as client:
            for index, media_url in enumerate(media_urls[:MEDIA_GROUP_LIMIT]):
                content, content_type = await self.download_media_file(client, media_url)
                media.append(
                    TelegramUpload(
                        field_name=f"media{index}",
                        filename=f"telegram-media-{index}.{SUPPORTED_MEDIA_CONTENT_TYPES[content_type]}",
                        content=content,
                        content_type=content_type,
                    )
                )

        return media

    async def download_media_file(self, client: httpx.AsyncClient, media_url: str) -> tuple[bytes, str]:
        """
        Download one preview media file with a strict in-memory size cap.
        """
        content = bytearray()
        async with client.stream("GET", media_url) as response:
            response.raise_for_status()
            raw_content_type = response.headers.get("content-type")
            if raw_content_type is None:
                raise UnsupportedPreviewMediaError("Telegram preview media response has no content type")

            content_type = self.extract_supported_content_type(raw_content_type)
            raw_content_length = response.headers.get("content-length")
            if raw_content_length is not None:
                if not raw_content_length.isdecimal():
                    raise UnsupportedPreviewMediaError("Telegram preview media response has invalid content length")
                if int(raw_content_length) > self.media_max_bytes:
                    raise UnsupportedPreviewMediaError("Telegram preview media response is too large")

            async for chunk in response.aiter_bytes():
                content.extend(chunk)
                if len(content) > self.media_max_bytes:
                    raise UnsupportedPreviewMediaError("Telegram preview media response is too large")

        return bytes(content), content_type

    def is_retryable_telegram_error(self, error: TelegramApiError) -> bool:
        """
        Return whether Telegram rejected delivery for a transient reason.
        """
        return error.error_code == 429 or error.error_code >= 500

    def extract_supported_content_type(self, raw_content_type: str) -> str:
        """
        Validate and normalize a downloaded media content type for Telegram photo upload.
        """
        content_type = raw_content_type.split(";", maxsplit=1)[0].lower()
        if content_type not in SUPPORTED_MEDIA_CONTENT_TYPES:
            raise UnsupportedPreviewMediaError(f"Unsupported Telegram preview media type: {content_type}")
        return content_type

    def build_message_text(self, event: PostReferenceEvent) -> str:
        """
        Build repost text with preserved HTML and a plain channel header.
        """
        channel_header = self.build_channel_header(event)
        if event.text_html:
            return f"{channel_header}\n\n{event.text_html}"
        return channel_header

    def build_channel_header(self, event: PostReferenceEvent) -> str:
        """
        Build a bold, underlined channel header that does not affect link previews.
        """
        display_name = html.escape(event.channel_display_name, quote=False)
        return f"<b><u>{display_name}</u></b>"

    def build_media_caption(self, event: PostReferenceEvent) -> str:
        """
        Build a valid Telegram HTML caption that fits media caption limits.
        """
        text_html = self.build_message_text(event)
        if len(text_html) <= PHOTO_CAPTION_LIMIT:
            return text_html

        return self.build_limited_plain_html_message(event, PHOTO_CAPTION_LIMIT)

    def build_text_message(self, event: PostReferenceEvent) -> str:
        """
        Build a valid Telegram HTML text message that fits text message limits.
        """
        text_html = self.build_message_text(event)
        if len(text_html) <= TEXT_MESSAGE_LIMIT:
            return text_html

        return self.build_limited_plain_html_message(event, TEXT_MESSAGE_LIMIT)

    def build_limited_plain_html_message(self, event: PostReferenceEvent, limit: int) -> str:
        """
        Build length-limited HTML without cutting through existing tags or entities.
        """
        channel_header = self.build_channel_header(event)
        reserved_length = len(channel_header) + 2
        body_limit = limit - reserved_length

        if body_limit <= 1:
            return channel_header

        body_text = self.html_to_plain_text(event.text_html)
        limited_body = self.limit_plain_text(body_text, body_limit)
        if limited_body:
            return f"{channel_header}\n\n{html.escape(limited_body, quote=False)}"
        return channel_header

    def html_to_plain_text(self, text_html: str) -> str:
        """
        Convert supported Telegram HTML into plain text for safe length limiting.
        """
        return BeautifulSoup(text_html, "html.parser").get_text().strip()

    def limit_plain_text(self, text: str, limit: int) -> str:
        """
        Keep plain text within a Telegram character limit without breaking HTML syntax.
        """
        if len(text) <= limit:
            return text

        return text[: limit - 1].rstrip() + "…"

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
