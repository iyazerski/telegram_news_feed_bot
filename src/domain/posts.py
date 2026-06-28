from dataclasses import dataclass


@dataclass(frozen=True)
class DiscoveredTelegramPost:
    source_channel: str
    channel_display_name: str
    message_id: int
    text_html: str
    media_urls: list[str]
    post_url: str
