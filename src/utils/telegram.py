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
