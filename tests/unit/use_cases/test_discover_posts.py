import httpx
import pytest
from nats.aio.client import Client

from src.config.configs import AppConfigs
from src.infrastructure.database.orm import Database
from src.infrastructure.telegram.errors import TelegramWebPreviewUnavailableError
from src.infrastructure.telegram.web_preview_client import TelegramWebPreviewClient
from src.use_cases.discover_posts import PublicChannelPoller
from src.use_cases.manage_channels import ChannelService

pytestmark = pytest.mark.unit


class RecordingPreviewClient(TelegramWebPreviewClient):
    def __init__(self) -> None:
        """
        Create a preview client that records requests and fails one channel.
        """
        self.requested_usernames: list[str] = []

    async def fetch_channel_preview(self, username: str) -> str:
        """
        Record a request, fail alpha, and return an empty preview for other channels.
        """
        self.requested_usernames.append(username)
        if username == "alpha":
            request = httpx.Request("GET", "https://telegram.me/s/alpha")
            error = httpx.ConnectError("Name or service not known", request=request)
            raise TelegramWebPreviewUnavailableError(username) from error
        return "<html></html>"


@pytest.mark.asyncio
async def test_run_once_defers_unavailable_channel_until_next_cycle(
    app_configs: AppConfigs,
    database: Database,
) -> None:
    """
    Verify an exhausted channel does not stop other channels and is retried next cycle.
    """
    with database.create_session() as session:
        channels = ChannelService()
        channels.add_channel(session, "alpha")
        channels.add_channel(session, "beta")
        session.commit()

    poller = PublicChannelPoller(app_configs, database)
    preview_client = RecordingPreviewClient()
    poller.preview_client = preview_client
    nats_client = Client()

    await poller.run_once(nats_client)
    await poller.run_once(nats_client)

    assert preview_client.requested_usernames == ["alpha", "beta", "alpha", "beta"]
