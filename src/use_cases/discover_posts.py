import asyncio

from loguru import logger
from nats.aio.client import Client

from src.config.configs import AppConfigs
from src.infrastructure.database.models import SourceChannel
from src.infrastructure.database.orm import Database
from src.infrastructure.messaging.events import PostReferenceBatchEvent
from src.infrastructure.telegram.errors import TelegramWebPreviewUnavailableError
from src.infrastructure.telegram.web_preview_client import TelegramWebPreviewClient
from src.infrastructure.telegram.web_preview_parser import TelegramWebPreviewParser
from src.use_cases.manage_channels import ChannelService

MAX_CONCURRENT_CHANNEL_POLLS = 10


class PublicChannelPoller:
    def __init__(self, configs: AppConfigs, db: Database) -> None:
        """
        Create the public Telegram preview polling business service.
        """
        self.configs = configs
        self.db = db
        self.channels = ChannelService()
        self.preview_client = TelegramWebPreviewClient(configs.poller_http_timeout_seconds)
        self.parser = TelegramWebPreviewParser()

    async def close(self) -> None:
        """
        Close the persistent Telegram preview HTTP client.
        """
        await self.preview_client.close()

    async def run_once(self, nats_client: Client) -> None:
        """
        Poll all active source channels once and publish newly discovered post references.
        """
        with self.db.create_session() as session:
            channels = self.channels.list_active_channels(session)

        semaphore = asyncio.Semaphore(MAX_CONCURRENT_CHANNEL_POLLS)
        async with asyncio.TaskGroup() as task_group:
            for channel in channels:
                task_group.create_task(self.poll_channel_safely(nats_client, channel, semaphore))

    async def poll_channel_safely(
        self,
        nats_client: Client,
        channel: SourceChannel,
        semaphore: asyncio.Semaphore,
    ) -> None:
        """
        Poll one channel within the concurrency limit and isolate availability failures.
        """
        async with semaphore:
            try:
                await self.poll_channel(nats_client, channel)
            except TelegramWebPreviewUnavailableError:
                logger.warning(f"Telegram web preview unavailable for @{channel.username}; retrying next poll cycle")

    async def poll_channel(self, nats_client: Client, channel: SourceChannel) -> None:
        """
        Poll a single public source channel and publish new post references.
        """
        html = await self.preview_client.fetch_channel_preview(channel.username)
        posts = self.parser.parse(channel.username, html)
        new_posts = [post for post in posts if post.message_id > channel.last_committed_message_id]

        if not new_posts:
            logger.debug(f"No new posts discovered for @{channel.username}")
            return

        # Keep one channel in one ordered batch so retries cannot skip later posts.
        event = PostReferenceBatchEvent.create(new_posts)
        await nats_client.publish(self.configs.nats_subject, event.model_dump_json().encode())

        logger.info(f"Published {len(new_posts)} post references for @{channel.username}")
