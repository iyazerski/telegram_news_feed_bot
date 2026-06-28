import html
from typing import Protocol

from bs4 import BeautifulSoup

PHOTO_CAPTION_LIMIT = 1024
TEXT_MESSAGE_LIMIT = 4096


class TelegramMessageSource(Protocol):
    channel_display_name: str
    text_html: str


class TelegramMessageFormatter:
    def build_media_caption(self, event: TelegramMessageSource) -> str:
        """
        Build a valid Telegram HTML caption that fits media caption limits.
        """
        text_html = self.build_message_text(event)
        if len(text_html) <= PHOTO_CAPTION_LIMIT:
            return text_html

        return self.build_limited_plain_html_message(event, PHOTO_CAPTION_LIMIT)

    def build_text_message(self, event: TelegramMessageSource) -> str:
        """
        Build a valid Telegram HTML text message that fits text message limits.
        """
        text_html = self.build_message_text(event)
        if len(text_html) <= TEXT_MESSAGE_LIMIT:
            return text_html

        return self.build_limited_plain_html_message(event, TEXT_MESSAGE_LIMIT)

    def build_message_text(self, event: TelegramMessageSource) -> str:
        """
        Build repost text with preserved HTML and a plain channel header.
        """
        channel_header = self.build_channel_header(event)
        if event.text_html:
            return f"{channel_header}\n\n{event.text_html}"
        return channel_header

    def build_channel_header(self, event: TelegramMessageSource) -> str:
        """
        Build a bold, underlined channel header that does not affect link previews.
        """
        display_name = html.escape(event.channel_display_name, quote=False)
        return f"<b><u>{display_name}</u></b>"

    def build_limited_plain_html_message(self, event: TelegramMessageSource, limit: int) -> str:
        """
        Build length-limited HTML without cutting through existing tags or entities.
        """
        channel_header = self.build_channel_header(event)
        reserved_length = len(channel_header) + 2
        body_limit = limit - reserved_length

        if body_limit <= 1:
            return channel_header

        body_text = self.html_to_plain_text(event.text_html)
        limited_body = self.limit_plain_text(body_text, body_limit)
        if limited_body:
            return f"{channel_header}\n\n{html.escape(limited_body, quote=False)}"
        return channel_header

    def html_to_plain_text(self, text_html: str) -> str:
        """
        Convert supported Telegram HTML into plain text for safe length limiting.
        """
        return BeautifulSoup(text_html, "html.parser").get_text().strip()

    def limit_plain_text(self, text: str, limit: int) -> str:
        """
        Keep plain text within a Telegram character limit without breaking HTML syntax.
        """
        if len(text) <= limit:
            return text

        return text[: limit - 1].rstrip() + "…"
