import httpx

from src.infrastructure.telegram.errors import TelegramWebPreviewUnavailableError

PRIMARY_PREVIEW_HOST = "t.me"
FALLBACK_PREVIEW_HOST = "telegram.me"


class TelegramWebPreviewClient:
    def __init__(self, timeout_seconds: float) -> None:
        """
        Create a client for public Telegram channel web preview pages.
        """
        self.timeout_seconds = timeout_seconds

    async def fetch_channel_preview(self, username: str) -> str:
        """
        Fetch a public Telegram channel web preview page with bounded host failover.
        """
        async with httpx.AsyncClient(timeout=self.timeout_seconds, follow_redirects=True) as client:
            # Fall back once when the primary Telegram preview host cannot be reached.
            try:
                return await self.fetch_from_host(client, PRIMARY_PREVIEW_HOST, username)
            except httpx.ConnectError:
                try:
                    return await self.fetch_from_host(client, FALLBACK_PREVIEW_HOST, username)
                except httpx.ConnectError as exc:
                    raise TelegramWebPreviewUnavailableError(username) from exc

    async def fetch_from_host(self, client: httpx.AsyncClient, host: str, username: str) -> str:
        """
        Fetch a public channel preview from one Telegram web host.
        """
        response = await client.get(f"https://{host}/s/{username}")
        response.raise_for_status()
        return response.text
