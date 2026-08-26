import pytest

from src.config.configs import AppConfigs
from src.entrypoints.bot.commands import HELP_TEXT, BotCommandHandler
from src.entrypoints.bot.runtime import BotRuntime
from src.infrastructure.database.orm import Database
from src.infrastructure.telegram.bot_api import TelegramBotApi
from src.use_cases.manage_settings import SettingsService

pytestmark = pytest.mark.unit


class RecordingTelegramApi(TelegramBotApi):
    def __init__(self) -> None:
        """
        Create a Telegram API fake that records command responses.
        """
        self.sent_messages: list[tuple[int | str, str]] = []

    async def send_text_message(self, chat_id: int | str, text: str) -> None:
        """
        Record one plain command response.
        """
        self.sent_messages.append((chat_id, text))


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


@pytest.mark.asyncio
async def test_runtime_ignores_unsupported_commands(
    app_configs: AppConfigs,
    database: Database,
    settings_service: SettingsService,
) -> None:
    """
    Verify unsupported text does not use an uninitialized command response.
    """
    app_configs = app_configs.model_copy(update={"admin_user_id": ""})
    telegram = RecordingTelegramApi()
    handler = BotCommandHandler(app_configs, database, settings_service)
    runtime = BotRuntime(app_configs, telegram, handler, database, settings_service)

    await runtime.handle_update(
        {
            "message": {
                "text": "/unknown",
                "from": {"id": 123},
                "chat": {"id": 456},
            }
        }
    )

    assert telegram.sent_messages == []
