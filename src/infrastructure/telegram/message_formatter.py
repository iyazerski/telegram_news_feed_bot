import html
from typing import Protocol

from bs4 import BeautifulSoup

PHOTO_CAPTION_LIMIT = 1024
TEXT_MESSAGE_LIMIT = 4096


class TelegramMessageSource(Protocol):
    channel_display_name: str
    text_html: str
    post_url: str
    media_unavailable: bool


class TelegramMessageFormatter:
    def build_media_caption(self, event: TelegramMessageSource) -> str:
        """
        Build a media caption and leave oversized text for follow-up messages.
        """
        message_html = self.build_message_text(event)
        if len(self.html_to_plain_text(message_html)) <= PHOTO_CAPTION_LIMIT:
            return message_html

        return self.build_channel_header(event)

    def build_text_messages(self, event: TelegramMessageSource) -> list[str]:
        """
        Build one or more complete Telegram HTML text messages without losing content.
        """
        message_html = self.build_message_text(event)
        if len(self.html_to_plain_text(message_html)) <= TEXT_MESSAGE_LIMIT:
            return [message_html]

        channel_header = self.build_channel_header(event)
        header_plain_text = self.html_to_plain_text(channel_header)
        body_text = self.html_to_plain_text(self.build_message_body(event))
        first_body_limit = TEXT_MESSAGE_LIMIT - len(header_plain_text) - 2
        if first_body_limit <= 0:
            return self.split_plain_text(self.html_to_plain_text(message_html), TEXT_MESSAGE_LIMIT)

        body_parts = self.split_plain_text(body_text, first_body_limit)
        if not body_parts:
            return [channel_header]

        messages = [f"{channel_header}\n\n{html.escape(body_parts[0], quote=False)}"]
        messages.extend(
            html.escape(body_part, quote=False)
            for body_part in self.split_plain_text(body_text[len(body_parts[0]) :], TEXT_MESSAGE_LIMIT)
        )
        return messages

    def build_body_messages(self, event: TelegramMessageSource) -> list[str]:
        """
        Build follow-up messages for an oversized media caption without repeating the header.
        """
        body_html = self.build_message_body(event)
        body_text = self.html_to_plain_text(body_html)

        if len(body_text) <= TEXT_MESSAGE_LIMIT:
            return [body_html]

        return [
            html.escape(body_part, quote=False) for body_part in self.split_plain_text(body_text, TEXT_MESSAGE_LIMIT)
        ]

    def build_message_text(self, event: TelegramMessageSource) -> str:
        """
        Build repost text with preserved HTML and a fallback source link.
        """
        channel_header = self.build_channel_header(event)
        return f"{channel_header}\n\n{self.build_message_body(event)}"

    def build_message_body(self, event: TelegramMessageSource) -> str:
        """
        Build the source body and append its URL when media is unavailable.
        """
        if event.text_html and not event.media_unavailable:
            return event.text_html

        source_link = html.escape(event.post_url, quote=False)
        if event.text_html:
            return f"{event.text_html}\n\n{source_link}"
        return source_link

    def build_channel_header(self, event: TelegramMessageSource) -> str:
        """
        Build a bold, underlined channel header that does not affect link previews.
        """
        display_name = html.escape(event.channel_display_name, quote=False)
        return f"<b><u>{display_name}</u></b>"

    def html_to_plain_text(self, text_html: str) -> str:
        """
        Convert supported Telegram HTML into plain text for safe length limiting.
        """
        return BeautifulSoup(text_html, "html.parser").get_text().strip()

    def split_plain_text(self, text: str, limit: int) -> list[str]:
        """
        Split plain text into complete chunks that fit a Telegram character limit.
        """
        if not text:
            return []

        return [text[index : index + limit] for index in range(0, len(text), limit)]
