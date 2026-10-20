import pytest

from src.domain.recaps import RecapStory, parse_thinking_level, select_new_stories

pytestmark = pytest.mark.unit


def build_story(url: str) -> RecapStory:
    """Create a recap story that differs from others only by its URL."""
    return RecapStory(emoji="📰", title="Title", summary="Summary.", source_name="Source", url=url)


def test_select_new_stories_skips_sent_and_repeated_articles_up_to_limit() -> None:
    """Treat tracking, fragment, case, and trailing-slash variants as the same article."""
    stories = [
        build_story("https://News.example/a/?utm_source=x#top"),
        build_story("https://news.example/b"),
        build_story("https://news.example/b/?fbclid=1"),
        build_story("https://news.example/c?id=7"),
        build_story("https://news.example/d"),
    ]

    selected = select_new_stories(stories, ["https://news.example/a"], limit=2)

    assert [story.url for story in selected] == ["https://news.example/b", "https://news.example/c?id=7"]


def test_parse_thinking_level_rejects_unknown_levels() -> None:
    """Accept Pi thinking levels and reject anything else read from storage."""
    assert parse_thinking_level("high") == "high"
    with pytest.raises(ValueError, match="Unsupported thinking level"):
        parse_thinking_level("extreme")
