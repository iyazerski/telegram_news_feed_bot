import asyncio

from src.bot.commands import BotCommandHandler
from src.bot.runtime import BotRuntime
from src.configs.configs import load_configs
from src.orm.database import Database
from src.services.logging import configure_logging
from src.services.settings import SettingsService
from src.services.telegram_api import TelegramBotApi


async def run() -> None:
    """
    Start the Telegram bot command microservice.
    """
    configs = load_configs()
    configure_logging(configs.log_level)
    db = Database(configs.database_url)
    settings = SettingsService()
    telegram = TelegramBotApi(configs.bot_token, configs.telegram_http_timeout_seconds)
    handler = BotCommandHandler(configs, db, settings)
    runtime = BotRuntime(configs, telegram, handler)
    await runtime.run_forever()


if __name__ == "__main__":
    asyncio.run(run())
