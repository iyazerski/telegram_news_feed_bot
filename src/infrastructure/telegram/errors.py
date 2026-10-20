class TelegramApiError(Exception):
    def __init__(self, method: str, description: str, error_code: int) -> None:
        """
        Create an exception describing a Telegram Bot API failure.
        """
        super().__init__(f"{method} failed: {description}")
        self.method = method
        self.description = description
        self.error_code = error_code

    @property
    def is_retryable(self) -> bool:
        """
        Return whether Telegram rejected the request for a transient reason.
        """
        return self.error_code == 429 or self.error_code >= 500


class TelegramWebPreviewUnavailableError(Exception):
    def __init__(self, username: str) -> None:
        """
        Create an exception for exhausted Telegram web preview connection attempts.
        """
        super().__init__(f"Telegram web preview is unavailable for @{username}")
        self.username = username
