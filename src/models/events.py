from dataclasses import dataclass
from datetime import UTC, datetime

from pydantic import BaseModel, ConfigDict


@dataclass(frozen=True)
class DiscoveredTelegramPost:
    source_channel: str
    channel_display_name: str
    message_id: int
    text_html: str
    media_urls: list[str]
    post_url: str


class PostReferenceEvent(BaseModel):
    model_config = ConfigDict(extra="forbid")

    source_channel: str
    channel_display_name: str
    message_id: int
    text_html: str
    media_urls: list[str]
    post_url: str
    discovered_at: datetime

    @classmethod
    def create(cls, post: DiscoveredTelegramPost) -> PostReferenceEvent:
        """
        Create a post reference event for a newly discovered Telegram message.
        """
        return cls(
            source_channel=post.source_channel,
            channel_display_name=post.channel_display_name,
            message_id=post.message_id,
            text_html=post.text_html,
            media_urls=post.media_urls,
            post_url=post.post_url,
            discovered_at=datetime.now(UTC),
        )
