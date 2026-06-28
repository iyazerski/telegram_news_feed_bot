import asyncio

from src.config.configs import load_configs
from src.config.logging import configure_logging
from src.entrypoints.dispatcher.runtime import DispatcherRuntime
from src.infrastructure.database.orm import Database
from src.use_cases.deliver_posts import TelegramForwardingService
from src.use_cases.manage_settings import SettingsService


async def run() -> None:
    """
    Start the Telegram forwarding dispatcher microservice.
    """
    configs = load_configs()
    configure_logging(configs.log_level)
    db = Database(configs.database_url)
    settings = SettingsService()
    forwarding = TelegramForwardingService(configs, db, settings)
    runtime = DispatcherRuntime(configs, forwarding)
    await runtime.run_forever()


if __name__ == "__main__":
    asyncio.run(run())
