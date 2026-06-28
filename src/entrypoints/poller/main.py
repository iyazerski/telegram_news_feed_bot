import asyncio

from src.config.configs import load_configs
from src.config.logging import configure_logging
from src.entrypoints.poller.runtime import PollerRuntime
from src.infrastructure.database.orm import Database
from src.use_cases.manage_settings import SettingsService


async def run() -> None:
    """
    Start the public Telegram channel poller microservice.
    """
    configs = load_configs()
    configure_logging(configs.log_level)
    db = Database(configs.database_url)
    settings = SettingsService()
    runtime = PollerRuntime(configs, db, settings)
    await runtime.run_forever()


if __name__ == "__main__":
    asyncio.run(run())
