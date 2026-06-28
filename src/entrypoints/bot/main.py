import asyncio

from src.config.configs import load_configs
from src.config.logging import configure_logging
from src.entrypoints.bot.commands import BotCommandHandler
from src.entrypoints.bot.runtime import BotRuntime
from src.infrastructure.database.orm import Database
from src.infrastructure.telegram.bot_api import TelegramBotApi
from src.use_cases.manage_settings import SettingsService


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
    runtime = BotRuntime(configs, telegram, handler, db, settings)
    await runtime.run_forever()


if __name__ == "__main__":
    asyncio.run(run())
