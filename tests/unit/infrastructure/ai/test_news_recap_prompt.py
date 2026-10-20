from datetime import UTC, datetime

import pytest

from src.domain.recaps import RecapSettings, RecapStory
from src.infrastructure.ai.news_recap_prompt import build_news_recap_prompt, parse_news_recap_output

pytestmark = pytest.mark.unit


def test_prompt_states_date_window_language_and_sent_stories() -> None:
    """Give the model the real date, research window, output language, and stories to skip."""
    settings = RecapSettings(
        topics=["AI", "Space"], language="Russian", interval_seconds=21_600, enabled=True, model="m", thinking="high"
    )

    prompt = build_news_recap_prompt(
        settings,
        datetime(2026, 10, 20, 18, 0, tzinfo=UTC),
        datetime(2026, 10, 19, 18, 0, tzinfo=UTC),
        5,
        [("Old story", "https://news.example/old")],
    )

    assert "Current date and time: 2026-10-20 18:00 UTC" in prompt
    assert "since 2026-10-19 18:00 UTC" in prompt
    assert "- AI\n- Space" in prompt
    assert "in Russian" in prompt
    assert "- Old story (https://news.example/old)" in prompt


def test_parse_output_tolerates_fences_and_rejects_invalid_links() -> None:
    """Read the JSON object inside surrounding text, and reject stories without an HTTP link."""
    output = """```json
    {"stories": [{"emoji": "🚀", "title": " Launch ", "summary": "It flew.", "source_name": "Reuters",
    "url": "https://reuters.example/launch"}]}
    ```"""

    assert parse_news_recap_output(output) == [
        RecapStory(
            emoji="🚀", title="Launch", summary="It flew.", source_name="Reuters", url="https://reuters.example/launch"
        )
    ]
    with pytest.raises(ValueError, match="not a valid recap"):
        parse_news_recap_output(output.replace("https://reuters.example/launch", "javascript:alert(1)"))
    with pytest.raises(ValueError, match="does not contain a JSON object"):
        parse_news_recap_output("No news today.")
