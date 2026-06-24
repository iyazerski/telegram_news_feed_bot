import asyncio

from src.configs.configs import load_configs
from src.dispatcher.runtime import DispatcherRuntime
from src.orm.database import Database
from src.services.dispatcher import TelegramForwardingService
from src.services.logging import configure_logging
from src.services.settings import SettingsService


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
