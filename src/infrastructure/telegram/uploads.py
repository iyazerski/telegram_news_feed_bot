from dataclasses import dataclass


@dataclass(frozen=True)
class TelegramUpload:
    """Represent one file uploaded through Telegram Bot API multipart form data."""

    field_name: str
    filename: str
    content: bytes
    content_type: str
