import httpx


class TelegramWebPreviewClient:
    def __init__(self, timeout_seconds: float) -> None:
        """
        Create a client for public Telegram channel web preview pages.
        """
        self.timeout_seconds = timeout_seconds

    async def fetch_channel_preview(self, username: str) -> str:
        """
        Fetch a public Telegram channel web preview page.
        """
        async with httpx.AsyncClient(timeout=self.timeout_seconds, follow_redirects=True) as client:
            response = await client.get(f"https://t.me/s/{username}")
            response.raise_for_status()
            return response.text
