import asyncio
from datetime import UTC, datetime

from loguru import logger

from src.config.configs import AppConfigs
from src.use_cases.generate_recaps import NewsRecapService


class RecapperRuntime:
    def __init__(self, configs: AppConfigs, recaps: NewsRecapService) -> None:
        """
        Create the AI news recap microservice runtime.
        """
        self.configs = configs
        self.recaps = recaps

    async def run_forever(self) -> None:
        """
        Check the recap schedule forever and send recaps when they are due.
        """
        logger.info("Recapper started")
        try:
            while True:
                now = datetime.now(UTC)
                try:
                    await self.recaps.refresh_models_if_stale(now)
                    await self.recaps.run_if_due(now)
                except Exception:
                    # Keep the service alive through transient database failures.
                    logger.exception("Recap check failed; retrying on the next check")
                await asyncio.sleep(self.configs.recap_check_interval_seconds)
        finally:
            await self.recaps.close()
