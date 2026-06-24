import unittest

import pytest

from src.bot.commands import BotCommandHandler
from src.configs.configs import AppConfigs
from src.models.database import Base
from src.orm.database import Database
from src.services.settings import SettingsService

pytestmark = pytest.mark.unit


class BotCommandHandlerTest(unittest.TestCase):
    def setUp(self) -> None:
        """
        Create an isolated command handler backed by an in-memory database.
        """
        self.db = Database("sqlite:///:memory:")
        Base.metadata.create_all(self.db.engine)
        self.handler = BotCommandHandler(AppConfigs(), self.db, SettingsService())

    def test_add_and_list_channel(self) -> None:
        """
        Verify channel commands persist and report a source subscription.
        """
        self.assertEqual("Added @example.", self.handler.handle_text("/add_channel https://t.me/example"))
        self.assertEqual(
            "Source channels:\n- @example (last committed: 0)",
            self.handler.handle_text("/list_channels"),
        )

    def test_status_reports_destination_and_interval(self) -> None:
        """
        Verify status reflects bot-chat destination and polling interval settings.
        """
        self.assertIn("This chat is now the feed destination.", self.handler.start(12345))
        self.assertEqual("Polling interval set to 300 seconds.", self.handler.handle_text("/set_interval 5m"))
        self.assertEqual(
            "Destination: this bot chat\nPolling interval: 300 seconds\nActive channels: 0",
            self.handler.handle_text("/status"),
        )
