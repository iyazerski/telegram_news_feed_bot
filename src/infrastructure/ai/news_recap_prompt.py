from datetime import datetime

from pydantic import BaseModel, ConfigDict, Field, ValidationError

from src.domain.recaps import RecapSettings, RecapStory


class RecapStoryOutput(BaseModel):
    model_config = ConfigDict(extra="ignore", str_strip_whitespace=True)

    emoji: str = Field(min_length=1, max_length=16)
    title: str = Field(min_length=1, max_length=300)
    summary: str = Field(min_length=1, max_length=1000)
    source_name: str = Field(min_length=1, max_length=120)
    url: str = Field(pattern=r"^https?://\S+$", max_length=2000)


class RecapOutput(BaseModel):
    model_config = ConfigDict(extra="ignore")

    stories: list[RecapStoryOutput]


def build_news_recap_prompt(
    settings: RecapSettings,
    now: datetime,
    since: datetime,
    story_count: int,
    sent_stories: list[tuple[str, str]],
) -> str:
    """Build the instructions that make Pi research and return one recap as JSON."""
    topics = "\n".join(f"- {topic}" for topic in settings.topics)
    sent = "\n".join(f"- {title} ({url})" for title, url in sent_stories) or "- none"
    # Models assume their training cutoff is today, so the prompt states the real date and window.
    return f"""You prepare a short news digest for a Telegram chat.

Current date and time: {now:%Y-%m-%d %H:%M} UTC. Trust this date over your own assumptions.
Time window: news published or substantially updated since {since:%Y-%m-%d %H:%M} UTC.

Topics:
{topics}

Steps:
1. Use web_search with several focused queries per topic. Include the current month and year in queries.
2. Pick the 3 to {story_count} most important stories in the time window across all topics. Rank them by impact and by how widely reputable outlets cover them. Cover several topics when each has important news, but never add a weak story only to fill a topic.
3. For every story, link one specific article from a reputable original source. Never link homepages, search pages, tag pages, or aggregators. Only use URLs that appeared in your search or fetch results. Use fetch_content when you need to confirm a date or detail.
4. Skip stories that repeat these already sent ones, even when another outlet reports them:
{sent}

Write title and summary in {settings.language}. Keep source_name as the outlet's own name.
Each summary has 2 short sentences: what happened, and why it matters.
Pick one emoji that fits each story.

Reply with only this JSON object, without Markdown fences or any other text:
{{"stories": [{{"emoji": "...", "title": "...", "summary": "...", "source_name": "...", "url": "https://..."}}]}}
Return {{"stories": []}} when nothing important and new happened in the time window."""


def parse_news_recap_output(output: str) -> list[RecapStory]:
    """Parse Pi's recap reply into stories, tolerating text around the JSON object."""
    start = output.find("{")
    end = output.rfind("}")
    if start == -1 or end < start:
        raise ValueError("Pi reply does not contain a JSON object")
    try:
        recap = RecapOutput.model_validate_json(output[start : end + 1])
    except ValidationError as exc:
        raise ValueError(f"Pi reply is not a valid recap: {exc}") from exc
    return [
        RecapStory(
            emoji=story.emoji,
            title=story.title,
            summary=story.summary,
            source_name=story.source_name,
            url=story.url,
        )
        for story in recap.stories
    ]
