import pytest

from src.config.configs import AppConfigs
from src.entrypoints.bot.commands import HELP_TEXT, BotCommandHandler
from src.infrastructure.database.orm import Database
from src.use_cases.manage_settings import SettingsService

pytestmark = pytest.mark.unit


@pytest.fixture
def command_handler(
    app_configs: AppConfigs, database: Database, settings_service: SettingsService
) -> BotCommandHandler:
    """
    Create an isolated command handler backed by an in-memory database.
    """
    return BotCommandHandler(app_configs, database, settings_service)


def test_start_sets_destination_and_returns_intro_text(command_handler: BotCommandHandler) -> None:
    """
    Verify start binds the destination chat and returns the product introduction.
    """
    chat_id = "12345"
    assert command_handler.start(chat_id) == HELP_TEXT
    with command_handler.db.create_session() as session:
        assert command_handler.settings.get_destination_chat_id(session) == chat_id
