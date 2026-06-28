import pytest

from src.infrastructure.telegram.web_preview_parser import TelegramWebPreviewParser

pytestmark = pytest.mark.unit


@pytest.fixture
def telegram_preview_html() -> str:
    """
    Create Telegram preview HTML containing matching and non-matching posts.
    """
    return """
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


def test_parse_extracts_matching_channel_message_metadata(telegram_preview_html: str) -> None:
    """
    Verify preview parsing returns structured posts from the requested source channel.
    """
    posts = TelegramWebPreviewParser().parse("example", telegram_preview_html)

    assert [post.message_id for post in posts] == [12, 14]
    assert posts[0].channel_display_name == "Example News"
    assert posts[0].post_url == "https://t.me/example/12"
    assert posts[0].media_urls == ["https://cdn.example/photo.jpg", "https://cdn.example/photo-2.jpg"]
    assert posts[0].text_html == 'Hello <b>bold</b>\n\n<a href="https://example.com">link</a>\n👉'
