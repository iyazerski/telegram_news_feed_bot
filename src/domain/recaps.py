from dataclasses import dataclass
from typing import Literal, get_args
from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit

# Tracking parameters differ between shares of one article, so they must not defeat deduplication.
TRACKING_QUERY_PREFIXES = ("utm_", "fbclid", "gclid", "mc_", "ref")

ThinkingLevel = Literal["off", "minimal", "low", "medium", "high", "xhigh", "max"]
THINKING_LEVELS: tuple[ThinkingLevel, ...] = get_args(ThinkingLevel)


@dataclass(frozen=True)
class RecapSettings:
    topics: list[str]
    language: str
    interval_seconds: int
    enabled: bool
    model: str
    thinking: ThinkingLevel


@dataclass(frozen=True)
class RecapStory:
    emoji: str
    title: str
    summary: str
    source_name: str
    url: str


def parse_thinking_level(value: str) -> ThinkingLevel:
    """Return a stored thinking level, rejecting values Pi does not accept."""
    for level in THINKING_LEVELS:
        if level == value:
            return level
    raise ValueError(f"Unsupported thinking level: {value}")


def normalize_story_url(url: str) -> str:
    """Return a comparable article URL without fragments, tracking parameters, or a trailing slash."""
    parts = urlsplit(url.strip())
    query = urlencode(
        [(key, value) for key, value in parse_qsl(parts.query) if not key.lower().startswith(TRACKING_QUERY_PREFIXES)]
    )
    return urlunsplit((parts.scheme.lower(), parts.netloc.lower(), parts.path.rstrip("/"), query, ""))


def select_new_stories(stories: list[RecapStory], sent_urls: list[str], limit: int) -> list[RecapStory]:
    """Keep up to the limit of stories whose articles were neither sent before nor repeated in this recap."""
    seen = {normalize_story_url(url) for url in sent_urls}
    selected: list[RecapStory] = []
    for story in stories:
        normalized_url = normalize_story_url(story.url)
        if normalized_url in seen:
            continue
        seen.add(normalized_url)
        selected.append(story)
        if len(selected) == limit:
            break
    return selected
