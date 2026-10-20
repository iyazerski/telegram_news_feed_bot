import asyncio

from src.config.configs import load_configs
from src.config.logging import configure_logging
from src.entrypoints.recapper.runtime import RecapperRuntime
from src.infrastructure.database.orm import Database
from src.use_cases.generate_recaps import NewsRecapService
from src.use_cases.manage_settings import SettingsService


async def run() -> None:
    """
    Start the AI news recap microservice.
    """
    configs = load_configs()
    configure_logging(configs.log_level)
    db = Database(configs.database_url)
    recaps = NewsRecapService(configs, db, SettingsService())
    runtime = RecapperRuntime(configs, recaps)
    await runtime.run_forever()


if __name__ == "__main__":
    asyncio.run(run())
