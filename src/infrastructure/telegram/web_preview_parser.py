import html
import re

from selectolax.lexbor import LexborHTMLParser, LexborNode

from src.domain.posts import DiscoveredTelegramPost

BACKGROUND_IMAGE_RE = re.compile(r"background-image:url\(['\"]?(?P<url>[^'\")]+)")


class TelegramWebPreviewParser:
    def parse(self, source_channel: str, html_text: str, after_message_id: int) -> list[DiscoveredTelegramPost]:
        """
        Extract structured Telegram post metadata from a public channel web preview page.
        """
        tree = LexborHTMLParser(html_text)
        posts: list[DiscoveredTelegramPost] = []

        # Telegram exposes public preview message IDs in data-post attributes.
        for message_node in tree.css("[data-post]"):
            data_post = message_node.attributes.get("data-post")
            if data_post is None:
                raise ValueError("Telegram data-post attribute is missing")

            channel, raw_message_id = self._parse_data_post(data_post)
            if channel == source_channel:
                message_id = int(raw_message_id)
                if message_id <= after_message_id:
                    continue
                media_urls, media_unavailable = self._extract_media_urls(message_node)
                posts.append(
                    DiscoveredTelegramPost(
                        source_channel=channel,
                        channel_display_name=self._extract_channel_display_name(message_node),
                        message_id=message_id,
                        text_html=self._extract_text_html(message_node),
                        media_urls=media_urls,
                        post_url=f"https://t.me/{channel}/{message_id}",
                        media_unavailable=media_unavailable,
                    )
                )

        return sorted(posts, key=lambda post: post.message_id)

    def _parse_data_post(self, data_post: str) -> tuple[str, str]:
        """
        Parse a Telegram data-post value into a channel username and message ID.
        """
        channel, raw_message_id = data_post.split("/", maxsplit=1)
        return channel.lower(), raw_message_id

    def _extract_channel_display_name(self, message_node: LexborNode) -> str:
        """
        Extract the Telegram channel display name from a preview message node.
        """
        owner_node = message_node.css_first(".tgme_widget_message_owner_name")
        if owner_node is None:
            raise ValueError("Telegram preview message owner name is missing")

        return owner_node.text(strip=True)

    def _extract_text_html(self, message_node: LexborNode) -> str:
        """
        Extract Telegram-compatible HTML text from a preview message node.
        """
        text_node = message_node.css_first(".js-message_text")
        if text_node is None:
            return ""

        rendered = self._render_children(text_node)
        return rendered.strip()

    def _render_children(self, node: LexborNode, in_pre: bool = False) -> str:
        """
        Render child nodes into the subset of HTML accepted by Telegram Bot API.
        """
        return "".join(self._render_node(child, in_pre) for child in node.iter(include_text=True))

    def _render_node(self, node: LexborNode, in_pre: bool) -> str:
        """
        Render one text or element node into Telegram-compatible HTML.
        """
        if node.is_text_node:
            text = node.text_content or ""
            # Collapse markup indentation between tags so it does not become visible text, except inside <pre>.
            if not in_pre and text.isspace():
                return "\n" if "\n" in text else " "
            return html.escape(text, quote=False)

        name = node.tag
        if name == "br":
            return "\n"

        # Telegram renders emoji as styled <i> tags whose text content is the real emoji.
        if name == "i" and "emoji" in self._class_names(node):
            return html.escape(node.text(), quote=False)

        inner_html = self._render_children(node, in_pre or name == "pre")

        if name == "a":
            href = node.attributes.get("href")
            if href is None:
                return inner_html
            return f'<a href="{html.escape(href, quote=True)}">{inner_html}</a>'

        if name in {"b", "strong"}:
            return f"<b>{inner_html}</b>"

        if name in {"i", "em"}:
            return f"<i>{inner_html}</i>"

        if name == "u":
            return f"<u>{inner_html}</u>"

        if name in {"s", "strike", "del"}:
            return f"<s>{inner_html}</s>"

        if name == "code":
            return f"<code>{inner_html}</code>"

        if name == "pre":
            return f"<pre>{inner_html}</pre>"

        return inner_html

    def _extract_media_urls(self, message_node: LexborNode) -> tuple[list[str], bool]:
        """
        Extract media URLs and identify media that Telegram did not expose.
        """
        media_urls: list[str] = []
        media_unavailable = False
        for media_node in message_node.css(".tgme_widget_message_photo_wrap, .tgme_widget_message_video_wrap"):
            if "tgme_widget_message_photo_wrap" in self._class_names(media_node):
                style = media_node.attributes.get("style")
                if style is None:
                    media_unavailable = True
                    continue

                match = BACKGROUND_IMAGE_RE.search(style)
                if match is None:
                    media_unavailable = True
                    continue

                media_urls.append(self._normalize_media_url(match.group("url")))
                continue

            video_node = media_node.css_first("video")
            if video_node is None:
                media_unavailable = True
                continue

            media_url = video_node.attributes.get("src")
            if media_url is not None:
                media_urls.append(self._normalize_media_url(media_url))
            else:
                media_unavailable = True

        if not media_urls and message_node.css_first(".message_media_not_supported_wrap") is not None:
            media_unavailable = True

        return media_urls, media_unavailable

    def _class_names(self, node: LexborNode) -> list[str]:
        """
        Return the whitespace-separated class names of an element node.
        """
        class_attribute = node.attributes.get("class")
        return class_attribute.split() if class_attribute else []

    def _normalize_media_url(self, media_url: str) -> str:
        """
        Normalize Telegram preview media URLs for Bot API delivery.
        """
        if media_url.startswith("//"):
            return f"https:{media_url}"
        return media_url
