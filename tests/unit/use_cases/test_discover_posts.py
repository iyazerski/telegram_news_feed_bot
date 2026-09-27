import asyncio

import httpx
import pytest
from nats.aio.client import Client

from src.config.configs import AppConfigs
from src.infrastructure.database.models import SourceChannel
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
        Record a request, fail alpha, return an unparsable page for broken, and an empty preview otherwise.
        """
        self.requested_usernames.append(username)
        if username == "broken":
            # A matching post without an owner name makes the parser reject the page.
            return '<div data-post="broken/1"></div>'
        if username == "alpha":
            request = httpx.Request("GET", "https://telegram.me/s/alpha")
            error = httpx.ConnectError("Name or service not known", request=request)
            raise TelegramWebPreviewUnavailableError(username) from error
        return "<html></html>"


class SlowFirstChannelPoller(PublicChannelPoller):
    def __init__(self, app_configs: AppConfigs, database: Database) -> None:
        """
        Create a poller whose first channel waits until a channel from a later page is polled.
        """
        super().__init__(app_configs, database)
        self.later_page_polled = asyncio.Event()

    async def poll_channel(self, _nats_client: Client, channel: SourceChannel) -> None:
        """
        Block the first channel until the last channel runs, without contacting Telegram or NATS.
        """
        if channel.username == "channel00":
            await self.later_page_polled.wait()
        if channel.username == "channel10":
            self.later_page_polled.set()


class ConcurrencyRecordingPoller(PublicChannelPoller):
    def __init__(self, app_configs: AppConfigs, database: Database) -> None:
        """
        Create a poller that records the number of simultaneous channel polls.
        """
        super().__init__(app_configs, database)
        self.active_polls = 0
        self.max_active_polls = 0
        self.polled_usernames: list[str] = []
        self.page_sizes: list[int] = []

    def load_channel_page(self, after: str) -> list[SourceChannel]:
        """Record the working set size of each database page."""
        channels = super().load_channel_page(after)
        self.page_sizes.append(len(channels))
        return channels

    async def poll_channel(self, _nats_client: Client, _channel: SourceChannel) -> None:
        """
        Record one bounded poll without contacting Telegram or NATS.
        """
        self.polled_usernames.append(_channel.username)
        self.active_polls += 1
        self.max_active_polls = max(self.max_active_polls, self.active_polls)
        await asyncio.sleep(0)
        self.active_polls -= 1


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


@pytest.mark.asyncio
async def test_run_once_limits_concurrent_channel_polls(
    app_configs: AppConfigs,
    database: Database,
) -> None:
    """
    Verify channel requests run concurrently without exceeding the fixed limit.
    """
    with database.create_session() as session:
        channels = ChannelService()
        for index in range(23):
            channels.add_channel(session, f"channel{index}")
        session.commit()

    poller = ConcurrencyRecordingPoller(app_configs, database)
    await poller.run_once(Client())
    await poller.close()

    assert poller.max_active_polls == 10
    assert poller.page_sizes == [10, 10, 3, 0]
    assert sorted(poller.polled_usernames) == sorted(f"channel{index}" for index in range(23))


@pytest.mark.asyncio
async def test_run_once_isolates_unexpected_channel_failures(
    app_configs: AppConfigs,
    database: Database,
) -> None:
    """
    Verify a channel whose preview cannot be parsed does not stop other channels.
    """
    with database.create_session() as session:
        channels = ChannelService()
        channels.add_channel(session, "beta")
        channels.add_channel(session, "broken")
        session.commit()

    poller = PublicChannelPoller(app_configs, database)
    preview_client = RecordingPreviewClient()
    poller.preview_client = preview_client

    await poller.run_once(Client())

    assert preview_client.requested_usernames == ["beta", "broken"]


@pytest.mark.asyncio
async def test_run_once_does_not_wait_for_slow_channel_before_next_page(
    app_configs: AppConfigs,
    database: Database,
) -> None:
    """
    Verify a slow channel frees other slots so channels from later pages still run.
    """
    with database.create_session() as session:
        channels = ChannelService()
        for index in range(11):
            channels.add_channel(session, f"channel{index:02d}")
        session.commit()

    poller = SlowFirstChannelPoller(app_configs, database)
    await asyncio.wait_for(poller.run_once(Client()), timeout=5)
    await poller.close()

    assert poller.later_page_polled.is_set()
