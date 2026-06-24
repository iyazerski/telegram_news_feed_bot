import unittest

import pytest

from src.bot.commands import BotCommandHandler
from src.configs.configs import AppConfigs
from src.models.database import Base
from src.orm.database import Database
from src.services.settings import SettingsService

pytestmark = pytest.mark.unit


class BotCommandHandlerTest(unittest.TestCase):
    intro_text = "\n".join(
        [
            "Your personal Telegram news feed is ready 📰",
            "Collect posts from public channels in one place, tune your sources, and keep the updates flowing automatically ✨",
            "Open the menu to manage your feed👇",
        ]
    )

    def setUp(self) -> None:
        """
        Create an isolated command handler backed by an in-memory database.
        """
        self.db = Database("sqlite:///:memory:")
        Base.metadata.create_all(self.db.engine)
        self.handler = BotCommandHandler(AppConfigs(), self.db, SettingsService())

    def test_unknown_command_returns_intro_text(self) -> None:
        """
        Verify unknown text returns the product introduction.
        """
        self.assertEqual(self.intro_text, self.handler.handle_text("/add_channel https://t.me/example"))

    def test_start_sets_destination_and_returns_intro_text(self) -> None:
        """
        Verify start binds the destination chat and returns the product introduction.
        """
        self.assertEqual(
            f"This chat is now the feed destination.\n\n{self.intro_text}",
            self.handler.start(12345),
        )
