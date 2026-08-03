import pytest

from src.config.configs import AppConfigs

pytestmark = pytest.mark.unit


def test_default_media_limit_supports_telegram_video_uploads() -> None:
    """
    Verify the default media cap covers Telegram's supported video upload size.
    """
    assert AppConfigs().dispatcher_media_max_bytes == 50_000_000
