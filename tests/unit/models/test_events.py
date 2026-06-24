import unittest
from datetime import UTC

import pytest

from src.models.events import DiscoveredTelegramPost, PostReferenceEvent

pytestmark = pytest.mark.unit


class PostReferenceEventTest(unittest.TestCase):
    def test_create_builds_forwardable_reference(self) -> None:
        """
        Verify event creation stores the source channel, message ID, and UTC timestamp.
        """
        post = DiscoveredTelegramPost(
            source_channel="example",
            channel_display_name="Example News",
            message_id=123,
            text_html="<b>Hello</b>",
            media_urls=["https://cdn.example/photo.jpg"],
            post_url="https://t.me/example/123",
        )
        event = PostReferenceEvent.create(post)

        self.assertEqual("example", event.source_channel)
        self.assertEqual("Example News", event.channel_display_name)
        self.assertEqual(123, event.message_id)
        self.assertEqual("<b>Hello</b>", event.text_html)
        self.assertEqual(["https://cdn.example/photo.jpg"], event.media_urls)
        self.assertEqual("https://t.me/example/123", event.post_url)
        self.assertEqual(UTC, event.discovered_at.tzinfo)
