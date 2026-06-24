import unittest

import pytest

from src.utils.telegram import normalize_channel_username, parse_duration_seconds

pytestmark = pytest.mark.unit


class TelegramUtilsTest(unittest.TestCase):
    def test_normalize_channel_username_accepts_url_and_at_prefix(self) -> None:
        """
        Verify Telegram channel identifiers normalize to lowercase usernames.
        """
        self.assertEqual("example", normalize_channel_username("https://t.me/Example"))
        self.assertEqual("example", normalize_channel_username("@Example"))

    def test_parse_duration_seconds_accepts_supported_units(self) -> None:
        """
        Verify compact interval strings convert to seconds.
        """
        self.assertEqual(300, parse_duration_seconds("5m"))
        self.assertEqual(7200, parse_duration_seconds("2h"))
        self.assertEqual(45, parse_duration_seconds("45"))
