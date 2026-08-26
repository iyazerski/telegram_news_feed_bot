from datetime import UTC, datetime

from pydantic import BaseModel, ConfigDict

from src.domain.posts import DiscoveredTelegramPost


class PostReferenceEvent(BaseModel):
    model_config = ConfigDict(extra="forbid")

    source_channel: str
    channel_display_name: str
    message_id: int
    text_html: str
    media_urls: list[str]
    post_url: str
    discovered_at: datetime
    media_unavailable: bool = False

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
            media_unavailable=post.media_unavailable,
        )


class PostReferenceBatchEvent(BaseModel):
    model_config = ConfigDict(extra="forbid")

    posts: list[PostReferenceEvent]

    @classmethod
    def create(cls, posts: list[DiscoveredTelegramPost]) -> PostReferenceBatchEvent:
        """
        Create one chronological batch for posts discovered from the same channel.
        """
        if not posts:
            raise ValueError("A post reference batch requires at least one post")

        source_channel = posts[0].source_channel
        if any(post.source_channel != source_channel for post in posts):
            raise ValueError("All posts in a reference batch must belong to the same channel")

        return cls(posts=[PostReferenceEvent.create(post) for post in posts])
