import httpx
from loguru import logger
from nats.aio.client import Client

from src.configs.configs import AppConfigs
from src.models.database import SourceChannel
from src.models.events import PostReferenceEvent
from src.orm.database import Database
from src.services.channels import ChannelService
from src.services.discovery import TelegramWebPreviewParser


class PublicChannelPoller:
    def __init__(self, configs: AppConfigs, db: Database) -> None:
        """
        Create the public Telegram preview polling business service.
        """
        self.configs = configs
        self.db = db
        self.channels = ChannelService()
        self.parser = TelegramWebPreviewParser()

    async def run_once(self, nats_client: Client) -> None:
        """
        Poll all active source channels once and publish newly discovered post references.
        """
        with self.db.create_session() as session:
            channels = self.channels.list_active_channels(session)

        for channel in channels:
            await self.poll_channel(nats_client, channel)

    async def poll_channel(self, nats_client: Client, channel: SourceChannel) -> None:
        """
        Poll a single public source channel and publish new post references.
        """
        html = await self.fetch_channel_preview(channel.username)
        posts = self.parser.parse(channel.username, html)
        new_posts = [post for post in posts if post.message_id > channel.last_committed_message_id]

        if not new_posts:
            logger.debug(f"No new posts discovered for @{channel.username}")
            return

        # Keep chronological publication order and let the dispatcher advance the committed cursor.
        for post in new_posts:
            event = PostReferenceEvent.create(post)
            await nats_client.publish(self.configs.nats_subject, event.model_dump_json().encode())

        logger.info(f"Published {len(new_posts)} post references for @{channel.username}")

    async def fetch_channel_preview(self, username: str) -> str:
        """
        Fetch a public Telegram channel web preview page.
        """
        async with httpx.AsyncClient(timeout=self.configs.poller_http_timeout_seconds, follow_redirects=True) as client:
            response = await client.get(f"https://t.me/s/{username}")
            response.raise_for_status()
            return response.text
