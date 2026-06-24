import unittest

import pytest

from src.services.discovery import TelegramWebPreviewParser

pytestmark = pytest.mark.unit


class TelegramWebPreviewParserTest(unittest.TestCase):
    def test_parse_extracts_matching_channel_message_metadata(self) -> None:
        """
        Verify preview parsing returns structured posts from the requested source channel.
        """
        html = """
        <section>
          <div class="tgme_widget_message" data-post="Example/12">
            <a class="tgme_widget_message_owner_name" href="https://t.me/example"><span>Example News</span></a>
            <a class="tgme_widget_message_photo_wrap" style="background-image:url('//cdn.example/photo.jpg')"></a>
            <a class="tgme_widget_message_photo_wrap" style="background-image:url('https://cdn.example/photo-2.jpg')"></a>
            <div class="tgme_widget_message_text js-message_text">
              Hello <b>bold</b><br>
              <a href="https://example.com">link</a>
              <i class="emoji"><b>👉</b></i>
            </div>
          </div>
          <div class="tgme_widget_message" data-post="other/99"></div>
          <div class="tgme_widget_message" data-post="example/14">
            <a class="tgme_widget_message_owner_name" href="https://t.me/example"><span>Example News</span></a>
            <div class="tgme_widget_message_text js-message_text">Second</div>
          </div>
        </section>
        """

        posts = TelegramWebPreviewParser().parse("example", html)

        self.assertEqual([12, 14], [post.message_id for post in posts])
        self.assertEqual("Example News", posts[0].channel_display_name)
        self.assertEqual("https://t.me/example/12", posts[0].post_url)
        self.assertEqual(["https://cdn.example/photo.jpg", "https://cdn.example/photo-2.jpg"], posts[0].media_urls)
        self.assertEqual('Hello <b>bold</b>\n\n<a href="https://example.com">link</a>\n👉', posts[0].text_html)
