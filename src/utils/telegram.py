from urllib.parse import urlparse


def normalize_channel_username(username_or_url: str) -> str:
    """
    Normalize a Telegram public channel username or URL into a lowercase username.
    """
    value = username_or_url.strip()
    if value.startswith(("http://", "https://")):
        parsed = urlparse(value)
        parts = [part for part in parsed.path.split("/") if part]
        if parsed.netloc not in {"t.me", "telegram.me"}:
            raise ValueError("Only t.me or telegram.me channel URLs are supported")
        value = parts[0]

    value = value.removeprefix("@").strip().lower()
    if not value:
        raise ValueError("Channel username is required")
    if "/" in value:
        raise ValueError("Channel username must not include a message path")

    return value


def parse_duration_seconds(value: str) -> int:
    """
    Parse a compact duration like 300, 5m, or 1h into seconds.
    """
    text = value.strip().lower()
    if text.endswith("m"):
        seconds = int(text[:-1]) * 60
    elif text.endswith("h"):
        seconds = int(text[:-1]) * 3600
    else:
        seconds = int(text)

    if seconds <= 0:
        raise ValueError("Interval must be positive")

    return seconds
