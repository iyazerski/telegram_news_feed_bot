from datetime import UTC, datetime

import pytest

from src.infrastructure.messaging.events import PostReferenceEvent
from src.infrastructure.telegram.message_formatter import TelegramMessageFormatter

pytestmark = pytest.mark.unit


def test_build_text_messages_preserves_all_text_when_message_exceeds_limit() -> None:
    """
    Verify oversized messages are split without an ellipsis or lost characters.
    """
    body = "a" * 5000
    event = PostReferenceEvent(
        source_channel="example",
        channel_display_name="Example News",
        message_id=42,
        text_html=body,
        media_urls=[],
        post_url="https://t.me/example/42",
        discovered_at=datetime.now(UTC),
    )

    formatter = TelegramMessageFormatter()
    messages = formatter.build_text_messages(event)
    plain_messages = [formatter.html_to_plain_text(message) for message in messages]

    assert "".join(plain_messages) == f"Example News\n\n{body}"
    assert all(len(message) <= 4096 for message in plain_messages)
    assert "…" not in "".join(plain_messages)
