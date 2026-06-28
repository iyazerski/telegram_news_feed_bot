from collections.abc import Callable
from dataclasses import dataclass
from datetime import UTC, datetime

import pytest

from src.config.configs import AppConfigs
from src.infrastructure.database.orm import Database
from src.infrastructure.messaging.events import PostReferenceEvent
from src.infrastructure.telegram.bot_api import TelegramBotApi
from src.infrastructure.telegram.media_downloader import TelegramMediaDownloader
from src.infrastructure.telegram.uploads import TelegramUpload
from src.use_cases.deliver_posts import TelegramForwardingService
from src.use_cases.manage_channels import ChannelService
from src.use_cases.manage_settings import SettingsService

pytestmark = pytest.mark.unit

EventFactory = Callable[[str, list[str]], PostReferenceEvent]


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


@dataclass(frozen=True)
class ForwardingHarness:
    """Represent a configured dispatcher service and its observed Telegram boundary."""

    db: Database
    settings: SettingsService
    service: TelegramForwardingService
    telegram: FakeTelegramApi


class FakeTelegramMediaDownloader(TelegramMediaDownloader):
    def __init__(self) -> None:
        """
        Create a fake media downloader without network configuration.
        """

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


@pytest.fixture
def forwarding_harness(
    app_configs: AppConfigs,
    database: Database,
    settings_service: SettingsService,
) -> ForwardingHarness:
    """
    Create an isolated forwarding service backed by an in-memory database.
    """
    service = TelegramForwardingService(app_configs, database, settings_service)
    telegram = FakeTelegramApi()
    service.telegram = telegram
    service.media_downloader = FakeTelegramMediaDownloader()

    with database.create_session() as session:
        ChannelService().add_channel(session, "example")
        settings_service.set_destination_chat_id(session, "@dest")
        session.commit()

    return ForwardingHarness(db=database, settings=settings_service, service=service, telegram=telegram)


@pytest.fixture
def event_factory() -> EventFactory:
    """
    Create structured post events for dispatcher tests.
    """

    def create_event(text_html: str, media_urls: list[str]) -> PostReferenceEvent:
        """
        Create one structured post event with configurable text and media.
        """
        return PostReferenceEvent(
            source_channel="example",
            channel_display_name="Example News",
            message_id=42,
            text_html=text_html,
            media_urls=media_urls,
            post_url="https://t.me/example/42",
            discovered_at=datetime.now(UTC),
        )

    return create_event


def assert_committed_message_id(db: Database, expected_message_id: int) -> None:
    """
    Verify the channel committed cursor value.
    """
    with db.create_session() as session:
        channel = ChannelService().get_active_channel(session, "example")
        assert channel.last_committed_message_id == expected_message_id


@pytest.mark.asyncio
async def test_forward_event_sends_text_message_and_commits_cursor(
    forwarding_harness: ForwardingHarness,
    event_factory: EventFactory,
) -> None:
    """
    Verify text-only source posts are reposted as one HTML message.
    """
    event = event_factory('Hello <a href="https://example.com">world</a>', [])

    result = await forwarding_harness.service.forward_event(event)

    assert result.action == "ack"
    assert forwarding_harness.telegram.sent_messages == [
        (
            "@dest",
            '<b><u>Example News</u></b>\n\nHello <a href="https://example.com">world</a>',
        )
    ]
    assert forwarding_harness.telegram.sent_photos == []
    assert forwarding_harness.telegram.sent_media_groups == []
    assert_committed_message_id(forwarding_harness.db, 42)


@pytest.mark.asyncio
async def test_forward_event_sends_photo_message_when_caption_fits(
    forwarding_harness: ForwardingHarness,
    event_factory: EventFactory,
) -> None:
    """
    Verify photo source posts are reposted as one photo message when the caption fits.
    """
    event = event_factory("<b>Hello</b>", ["https://cdn.example/photo.jpg"])

    result = await forwarding_harness.service.forward_event(event)

    assert result.action == "ack"
    assert forwarding_harness.telegram.sent_photos == [
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
    ]
    assert forwarding_harness.telegram.sent_messages == []
    assert forwarding_harness.telegram.sent_media_groups == []
    assert_committed_message_id(forwarding_harness.db, 42)


@pytest.mark.asyncio
async def test_forward_event_sends_media_group_for_multiple_images(
    forwarding_harness: ForwardingHarness,
    event_factory: EventFactory,
) -> None:
    """
    Verify multi-image source posts are reposted as one grouped album send.
    """
    event = event_factory(
        '<b>Hello</b> <a href="https://example.com">world</a>',
        ["https://cdn.example/one.jpg", "https://cdn.example/two.jpg"],
    )

    result = await forwarding_harness.service.forward_event(event)

    assert result.action == "ack"
    assert forwarding_harness.telegram.sent_messages == []
    assert forwarding_harness.telegram.sent_photos == []
    assert forwarding_harness.telegram.sent_media_groups == [
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
            '<b><u>Example News</u></b>\n\n<b>Hello</b> <a href="https://example.com">world</a>',
        )
    ]
    assert_committed_message_id(forwarding_harness.db, 42)


@pytest.mark.asyncio
async def test_forward_event_skips_already_committed_message(
    forwarding_harness: ForwardingHarness,
    event_factory: EventFactory,
) -> None:
    """
    Verify rediscovered old messages are ignored without reposting.
    """
    with forwarding_harness.db.create_session() as session:
        channel = ChannelService().get_active_channel(session, "example")
        channel.last_committed_message_id = 42
        session.commit()

    event = event_factory("Hello", [])

    result = await forwarding_harness.service.forward_event(event)

    assert result.action == "ack"
    assert forwarding_harness.telegram.sent_messages == []
    assert forwarding_harness.telegram.sent_photos == []
    assert forwarding_harness.telegram.sent_media_groups == []
