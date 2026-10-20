import pytest

from src.domain.recaps import RecapStory
from src.infrastructure.telegram.message_formatter import TEXT_MESSAGE_LIMIT
from src.infrastructure.telegram.recap_formatter import EMPTY_RECAP_TEXT, NewsRecapFormatter

pytestmark = pytest.mark.unit


def test_build_message_escapes_story_text_and_links_sources() -> None:
    """Render each story as an escaped bold title, summary, and linked source name."""
    story = RecapStory(
        emoji="🤖",
        title="A <b> & B",
        summary="Why it matters > hype.",
        source_name="News & Co",
        url='https://news.example/a?x=1&y="2"',
    )

    message = NewsRecapFormatter().build_message([story], ["AI", "R&D"])

    assert message == (
        "🗞 <b>News recap</b>\n<i>AI · R&amp;D</i>\n\n"
        "🤖 <b>A &lt;b&gt; &amp; B</b>\n"
        "Why it matters &gt; hype.\n"
        '🔗 <a href="https://news.example/a?x=1&amp;y=&quot;2&quot;">News &amp; Co</a>'
    )


def test_build_message_reports_empty_recaps_and_drops_overflowing_stories() -> None:
    """Say when nothing is new, and keep long recaps within one Telegram message."""
    formatter = NewsRecapFormatter()
    assert formatter.build_message([], ["AI"]).endswith(EMPTY_RECAP_TEXT)

    stories = [
        RecapStory(emoji="📰", title=f"Story {index}", summary="x" * 1000, source_name="S", url="https://s.example")
        for index in range(5)
    ]
    message = formatter.build_message(stories, ["AI"])

    assert len(message) <= TEXT_MESSAGE_LIMIT
    assert "Story 2" in message
    assert "Story 3" not in message
