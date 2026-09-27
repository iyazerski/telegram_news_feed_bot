import asyncio
from typing import Any

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


class ScriptedUpdatesTelegramApi(RecordingTelegramApi):
    def __init__(self, updates: list[dict[str, Any]]) -> None:
        """
        Create a Telegram API fake that returns one update batch and then stops polling.
        """
        super().__init__()
        self.updates = updates
        self.requested_offsets: list[int] = []

    async def get_updates(self, offset: int, _timeout_seconds: int) -> list[dict[str, Any]]:
        """
        Record the requested offset, return scripted updates once, and then cancel the loop.
        """
        self.requested_offsets.append(offset)
        if len(self.requested_offsets) > 1:
            raise asyncio.CancelledError
        return self.updates


class FailingFirstChatCommandHandler(BotCommandHandler):
    def start(self, chat_id: int | str) -> str:
        """
        Fail the first chat as a database outage would and handle other chats normally.
        """
        if chat_id == 1:
            raise RuntimeError("database is unavailable")
        return super().start(chat_id)


@pytest.fixture
def command_handler(database: Database, settings_service: SettingsService) -> BotCommandHandler:
    """
    Create an isolated command handler backed by an in-memory database.
    """
    return BotCommandHandler(database, settings_service)


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
    handler = BotCommandHandler(database, settings_service)
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


def create_start_update(update_id: int, chat_id: int) -> dict[str, Any]:
    """
    Create one Telegram /start update for a chat.
    """
    return {"update_id": update_id, "message": {"text": "/start", "from": {"id": 123}, "chat": {"id": chat_id}}}


@pytest.mark.asyncio
async def test_polling_isolates_failed_update_and_polls_again_immediately(
    app_configs: AppConfigs,
    database: Database,
    settings_service: SettingsService,
) -> None:
    """
    Verify one failing update does not stop polling and the next long poll starts without a delay.
    """
    app_configs = app_configs.model_copy(update={"admin_user_id": "", "telegram_get_updates_retry_seconds": 60})
    telegram = ScriptedUpdatesTelegramApi([create_start_update(10, 1), create_start_update(11, 2)])
    handler = FailingFirstChatCommandHandler(database, settings_service)
    runtime = BotRuntime(app_configs, telegram, handler, database, settings_service)

    with pytest.raises(asyncio.CancelledError):
        await asyncio.wait_for(runtime.run_polling_forever(), timeout=5)

    assert telegram.sent_messages == [(2, HELP_TEXT)]
    assert telegram.requested_offsets == [0, 12]
