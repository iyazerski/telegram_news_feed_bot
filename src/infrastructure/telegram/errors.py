class TelegramApiError(Exception):
    def __init__(self, method: str, description: str, error_code: int) -> None:
        """
        Create an exception describing a Telegram Bot API failure.
        """
        super().__init__(f"{method} failed: {description}")
        self.method = method
        self.description = description
        self.error_code = error_code
