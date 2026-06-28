from datetime import UTC

import pytest

from src.domain.posts import DiscoveredTelegramPost
from src.infrastructure.messaging.events import PostReferenceEvent

pytestmark = pytest.mark.unit


@pytest.fixture
def discovered_post() -> DiscoveredTelegramPost:
    """
    Create a discovered Telegram post for event model tests.
    """
    return DiscoveredTelegramPost(
        source_channel="example",
        channel_display_name="Example News",
        message_id=123,
        text_html="<b>Hello</b>",
        media_urls=["https://cdn.example/photo.jpg"],
        post_url="https://t.me/example/123",
    )


def test_create_builds_forwardable_reference(discovered_post: DiscoveredTelegramPost) -> None:
    """
    Verify event creation stores the source channel, message ID, and UTC timestamp.
    """
    event = PostReferenceEvent.create(discovered_post)

    assert event.source_channel == "example"
    assert event.channel_display_name == "Example News"
    assert event.message_id == 123
    assert event.text_html == "<b>Hello</b>"
    assert event.media_urls == ["https://cdn.example/photo.jpg"]
    assert event.post_url == "https://t.me/example/123"
    assert event.discovered_at.tzinfo == UTC
