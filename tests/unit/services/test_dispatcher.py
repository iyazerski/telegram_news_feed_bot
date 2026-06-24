import unittest
from datetime import UTC, datetime

import pytest

from src.configs.configs import AppConfigs
from src.models.database import Base
from src.models.events import PostReferenceEvent
from src.models.media import TelegramUpload
from src.orm.database import Database
from src.services.channels import ChannelService
from src.services.dispatcher import TelegramForwardingService
from src.services.settings import SettingsService
from src.services.telegram_api import TelegramBotApi

pytestmark = pytest.mark.unit


class FakeTelegramApi(TelegramBotApi):
    def __init__(self) -> None:
        """
        Create a fake Telegram API that records outgoing bot messages.
        """
        self.sent_messages: list[tuple[int | str, str]] = []
        self.sent_photos: list[tuple[int | str, TelegramUpload, str]] = []
        self.sent_media_groups: list[tuple[int | str, list[TelegramUpload], str]] = []

    async def send_html_message(self, chat_id: int | str, text: str) -> None:
        """
        Record a sent HTML text message request.
        """
        self.sent_messages.append((chat_id, text))

    async def send_photo(self, chat_id: int | str, photo: TelegramUpload, caption: str) -> None:
        """
        Record a sent photo message request.
        """
        self.sent_photos.append((chat_id, photo, caption))

    async def send_media_group(self, chat_id: int | str, photos: list[TelegramUpload], caption: str) -> None:
        """
        Record a sent media group request.
        """
        self.sent_media_groups.append((chat_id, photos, caption))


class FakeTelegramForwardingService(TelegramForwardingService):
    async def download_media(self, media_urls: list[str]) -> list[TelegramUpload]:
        """
        Create deterministic uploaded media for dispatcher tests.
        """
        media: list[TelegramUpload] = []
        for index, _media_url in enumerate(media_urls):
            media.append(
                TelegramUpload(
                    field_name=f"media{index}",
                    filename=f"telegram-media-{index}.jpg",
                    content=f"photo-{index}".encode(),
                    content_type="image/jpeg",
                )
            )
        return media


class TelegramForwardingServiceTest(unittest.IsolatedAsyncioTestCase):
    def setUp(self) -> None:
        """
        Create an isolated forwarding service backed by an in-memory database.
        """
        self.db = Database("sqlite:///:memory:")
        Base.metadata.create_all(self.db.engine)
        self.settings = SettingsService()
        self.service = FakeTelegramForwardingService(AppConfigs(), self.db, self.settings)
        self.telegram = FakeTelegramApi()
        self.service.telegram = self.telegram

        with self.db.create_session() as session:
            ChannelService().add_channel(session, "example")
            self.settings.set_destination_chat_id(session, "@dest")
            session.commit()

    async def test_forward_event_sends_text_message_and_commits_cursor(self) -> None:
        """
        Verify text-only source posts are reposted as one HTML message.
        """
        event = self.create_event(text_html='Hello <a href="https://example.com">world</a>')

        result = await self.service.forward_event(event)

        self.assertEqual("ack", result.action)
        self.assertEqual(
            [
                (
                    "@dest",
                    ('<b><u>Example News</u></b>\n\nHello <a href="https://example.com">world</a>'),
                )
            ],
            self.telegram.sent_messages,
        )
        self.assertEqual([], self.telegram.sent_photos)
        self.assertEqual([], self.telegram.sent_media_groups)
        self.assert_committed_message_id(42)

    async def test_forward_event_sends_photo_message_when_caption_fits(self) -> None:
        """
        Verify photo source posts are reposted as one photo message when the caption fits.
        """
        event = self.create_event(text_html="<b>Hello</b>", media_urls=["https://cdn.example/photo.jpg"])

        result = await self.service.forward_event(event)

        self.assertEqual("ack", result.action)
        self.assertEqual(
            [
                (
                    "@dest",
                    TelegramUpload(
                        field_name="media0",
                        filename="telegram-media-0.jpg",
                        content=b"photo-0",
                        content_type="image/jpeg",
                    ),
                    "<b><u>Example News</u></b>\n\n<b>Hello</b>",
                )
            ],
            self.telegram.sent_photos,
        )
        self.assertEqual([], self.telegram.sent_messages)
        self.assertEqual([], self.telegram.sent_media_groups)
        self.assert_committed_message_id(42)

    async def test_forward_event_sends_media_group_for_multiple_images(self) -> None:
        """
        Verify multi-image source posts are reposted as one grouped album send.
        """
        event = self.create_event(
            text_html='<b>Hello</b> <a href="https://example.com">world</a>',
            media_urls=["https://cdn.example/one.jpg", "https://cdn.example/two.jpg"],
        )

        result = await self.service.forward_event(event)

        self.assertEqual("ack", result.action)
        self.assertEqual([], self.telegram.sent_messages)
        self.assertEqual([], self.telegram.sent_photos)
        self.assertEqual(
            [
                (
                    "@dest",
                    [
                        TelegramUpload(
                            field_name="media0",
                            filename="telegram-media-0.jpg",
                            content=b"photo-0",
                            content_type="image/jpeg",
                        ),
                        TelegramUpload(
                            field_name="media1",
                            filename="telegram-media-1.jpg",
                            content=b"photo-1",
                            content_type="image/jpeg",
                        ),
                    ],
                    ('<b><u>Example News</u></b>\n\n<b>Hello</b> <a href="https://example.com">world</a>'),
                )
            ],
            self.telegram.sent_media_groups,
        )
        self.assert_committed_message_id(42)

    async def test_forward_event_skips_already_committed_message(self) -> None:
        """
        Verify rediscovered old messages are ignored without reposting.
        """
        with self.db.create_session() as session:
            channel = ChannelService().get_active_channel(session, "example")
            channel.last_committed_message_id = 42
            session.commit()

        event = self.create_event()

        result = await self.service.forward_event(event)

        self.assertEqual("ack", result.action)
        self.assertEqual([], self.telegram.sent_messages)
        self.assertEqual([], self.telegram.sent_photos)
        self.assertEqual([], self.telegram.sent_media_groups)

    def create_event(self, text_html: str = "Hello", media_urls: list[str] | None = None) -> PostReferenceEvent:
        """
        Create a structured post event for dispatcher tests.
        """
        event_media_urls = media_urls
        if event_media_urls is None:
            event_media_urls = []

        return PostReferenceEvent(
            source_channel="example",
            channel_display_name="Example News",
            message_id=42,
            text_html=text_html,
            media_urls=event_media_urls,
            post_url="https://t.me/example/42",
            discovered_at=datetime.now(UTC),
        )

    def assert_committed_message_id(self, expected_message_id: int) -> None:
        """
        Verify the channel committed cursor value.
        """
        with self.db.create_session() as session:
            channel = ChannelService().get_active_channel(session, "example")
            self.assertEqual(expected_message_id, channel.last_committed_message_id)
