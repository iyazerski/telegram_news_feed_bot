import unittest

import pytest

from src.utils.telegram import normalize_channel_username

pytestmark = pytest.mark.unit


class TelegramUtilsTest(unittest.TestCase):
    def test_normalize_channel_username_accepts_url_and_at_prefix(self) -> None:
        """
        Verify Telegram channel identifiers normalize to lowercase usernames.
        """
        self.assertEqual("example", normalize_channel_username("https://t.me/Example"))
        self.assertEqual("example", normalize_channel_username("@Example"))
