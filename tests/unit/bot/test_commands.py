import pytest

from src.bot.commands import BotCommandHandler
from src.configs.configs import AppConfigs
from src.orm.database import Database
from src.services.settings import SettingsService

pytestmark = pytest.mark.unit

INTRO_TEXT = "\n".join(
    [
        "Your personal Telegram news feed is ready 📰",
        "Collect posts from public channels in one place, tune your sources, and keep the updates flowing automatically ✨",
        "Open the menu to manage your feed👇",
    ]
)


@pytest.fixture
def command_handler(
    app_configs: AppConfigs, database: Database, settings_service: SettingsService
) -> BotCommandHandler:
    """
    Create an isolated command handler backed by an in-memory database.
    """
    return BotCommandHandler(app_configs, database, settings_service)


def test_unknown_command_returns_intro_text(command_handler: BotCommandHandler) -> None:
    """
    Verify unknown text returns the product introduction.
    """
    assert command_handler.handle_text("/add_channel https://t.me/example") == INTRO_TEXT


def test_start_sets_destination_and_returns_intro_text(command_handler: BotCommandHandler) -> None:
    """
    Verify start binds the destination chat and returns the product introduction.
    """
    assert command_handler.start(12345) == f"This chat is now the feed destination.\n\n{INTRO_TEXT}"
