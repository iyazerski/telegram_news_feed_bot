import pytest

from src.infrastructure.telegram.usernames import normalize_channel_username

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


@pytest.mark.parametrize("username_or_url", ["https://t.me", "https://t.me/", "https://t.me/example/42"])
def test_normalize_channel_username_rejects_invalid_url_paths(username_or_url: str) -> None:
    """
    Verify Telegram URLs require one channel username path segment.
    """
    with pytest.raises(ValueError, match="Telegram channel URL"):
        normalize_channel_username(username_or_url)
