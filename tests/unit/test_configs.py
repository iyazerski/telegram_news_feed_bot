import pytest

from src.config.configs import AppConfigs

pytestmark = pytest.mark.unit


def test_default_media_limit_supports_telegram_video_uploads() -> None:
    """
    Verify the default media cap covers Telegram's supported video upload size.
    """
    assert AppConfigs().dispatcher_post_media_max_bytes == 50_000_000


def test_default_nats_subject_uses_batch_event_version() -> None:
    """
    Verify the default subject isolates batch events from the old wire format.
    """
    assert AppConfigs.model_fields["nats_subject"].default == "telegram.posts.discovered.v2"
