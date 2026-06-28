import html
import re

from bs4 import BeautifulSoup, NavigableString, Tag

from src.domain.posts import DiscoveredTelegramPost

BACKGROUND_IMAGE_RE = re.compile(r"background-image:url\(['\"]?(?P<url>[^'\")]+)")


class TelegramWebPreviewParser:
    def parse(self, source_channel: str, html_text: str) -> list[DiscoveredTelegramPost]:
        """
        Extract structured Telegram post metadata from a public channel web preview page.
        """
        soup = BeautifulSoup(html_text, "html.parser")
        posts: list[DiscoveredTelegramPost] = []

        # Telegram exposes public preview message IDs in data-post attributes.
        for message_node in soup.select("[data-post]"):
            data_post = message_node["data-post"]
            if not isinstance(data_post, str):
                raise ValueError("Telegram data-post attribute must be a string")

            channel, raw_message_id = self._parse_data_post(data_post)
            if channel == source_channel:
                message_id = int(raw_message_id)
                posts.append(
                    DiscoveredTelegramPost(
                        source_channel=channel,
                        channel_display_name=self._extract_channel_display_name(message_node),
                        message_id=message_id,
                        text_html=self._extract_text_html(message_node),
                        media_urls=self._extract_media_urls(message_node),
                        post_url=f"https://t.me/{channel}/{message_id}",
                    )
                )

        return sorted(posts, key=lambda post: post.message_id)

    def _parse_data_post(self, data_post: str) -> tuple[str, str]:
        """
        Parse a Telegram data-post value into a channel username and message ID.
        """
        channel, raw_message_id = data_post.split("/", maxsplit=1)
        return channel.lower(), raw_message_id

    def _extract_channel_display_name(self, message_node: Tag) -> str:
        """
        Extract the Telegram channel display name from a preview message node.
        """
        owner_node = message_node.select_one(".tgme_widget_message_owner_name")
        if owner_node is None:
            raise ValueError("Telegram preview message owner name is missing")

        return owner_node.get_text(strip=True)

    def _extract_text_html(self, message_node: Tag) -> str:
        """
        Extract Telegram-compatible HTML text from a preview message node.
        """
        text_node = message_node.select_one(".js-message_text")
        if text_node is None:
            return ""

        rendered = self._render_children(text_node)
        return rendered.strip()

    def _render_children(self, node: Tag) -> str:
        """
        Render child nodes into the subset of HTML accepted by Telegram Bot API.
        """
        return "".join(self._render_child(child) for child in node.children)

    def _render_child(self, node: object) -> str:
        """
        Render a supported BeautifulSoup child node.
        """
        if isinstance(node, Tag | NavigableString):
            return self._render_node(node)
        return ""

    def _render_node(self, node: Tag | NavigableString) -> str:
        """
        Render one BeautifulSoup node into Telegram-compatible HTML.
        """
        if isinstance(node, NavigableString):
            return html.escape(str(node), quote=False)

        if node.name == "br":
            return "\n"

        # Telegram renders emoji as styled <i> tags whose text content is the real emoji.
        class_names = node.get_attribute_list("class")
        if node.name == "i" and "emoji" in class_names:
            return html.escape(node.get_text(), quote=False)

        inner_html = self._render_children(node)

        if node.name == "a":
            href = node.get("href")
            if not isinstance(href, str):
                return inner_html
            return f'<a href="{html.escape(href, quote=True)}">{inner_html}</a>'

        if node.name in {"b", "strong"}:
            return f"<b>{inner_html}</b>"

        if node.name in {"i", "em"}:
            return f"<i>{inner_html}</i>"

        if node.name == "u":
            return f"<u>{inner_html}</u>"

        if node.name in {"s", "strike", "del"}:
            return f"<s>{inner_html}</s>"

        if node.name == "code":
            return f"<code>{inner_html}</code>"

        if node.name == "pre":
            return f"<pre>{inner_html}</pre>"

        return inner_html

    def _extract_media_urls(self, message_node: Tag) -> list[str]:
        """
        Extract image URLs from a preview message node.
        """
        media_urls: list[str] = []
        for photo_node in message_node.select(".tgme_widget_message_photo_wrap"):
            style = photo_node.get("style")
            if not isinstance(style, str):
                continue

            match = BACKGROUND_IMAGE_RE.search(style)
            if match is None:
                continue

            media_urls.append(self._normalize_media_url(match.group("url")))

        return media_urls

    def _normalize_media_url(self, media_url: str) -> str:
        """
        Normalize Telegram preview media URLs for Bot API delivery.
        """
        if media_url.startswith("//"):
            return f"https:{media_url}"
        return media_url
