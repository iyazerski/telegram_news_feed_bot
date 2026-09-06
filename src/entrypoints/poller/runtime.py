import asyncio

import nats
from loguru import logger

from src.config.configs import AppConfigs
from src.infrastructure.database.orm import Database
from src.use_cases.discover_posts import PublicChannelPoller
from src.use_cases.manage_settings import SettingsService


class PollerRuntime:
    def __init__(self, configs: AppConfigs, db: Database, settings: SettingsService) -> None:
        """
        Create the polling microservice runtime.
        """
        self.configs = configs
        self.db = db
        self.settings = settings
        self.poller = PublicChannelPoller(configs, db)

    async def run_forever(self) -> None:
        """
        Run Telegram source polling cycles forever.
        """
        nats_client = await nats.connect(self.configs.nats_url)
        logger.info("Poller connected to NATS")

        try:
            while True:
                await self.poller.run_once(nats_client)
                await asyncio.sleep(await asyncio.to_thread(self.load_poll_interval_seconds))
        finally:
            await nats_client.close()
            await self.poller.close()

    def load_poll_interval_seconds(self) -> int:
        """
        Load the current poll interval from persistent settings.
        """
        with self.db.create_session() as session:
            return self.settings.get_poll_interval_seconds(session, self.configs.default_poll_interval_seconds)
