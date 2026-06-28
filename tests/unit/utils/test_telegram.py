import pytest

from src.utils.telegram import normalize_channel_username

pytestmark = pytest.mark.unit


@pytest.mark.parametrize(
    ("username_or_url", "expected_username"),
    [
        ("https://t.me/Example", "example"),
        ("@Example", "example"),
    ],
)
def test_normalize_channel_username_accepts_url_and_at_prefix(
    username_or_url: str,
    expected_username: str,
) -> None:
    """
    Verify Telegram channel identifiers normalize to lowercase usernames.
    """
    assert normalize_channel_username(username_or_url) == expected_username
