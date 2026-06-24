import hashlib
import hmac
import json
import time
from urllib.parse import parse_qsl

from src.bot.mini_app.models import TelegramMiniAppSession, TelegramMiniAppUser


def validate_mini_app_init_data(init_data: str, bot_token: str, max_age_seconds: int) -> TelegramMiniAppSession:
    """
    Validate signed Telegram Mini App init data and return the authenticated session.
    """
    values = dict(parse_qsl(init_data, keep_blank_values=True, strict_parsing=True))
    if "hash" not in values:
        raise ValueError("Telegram Mini App init data is missing its signature")
    if "auth_date" not in values:
        raise ValueError("Telegram Mini App init data is missing its auth date")
    if "user" not in values:
        raise ValueError("Telegram Mini App init data is missing its user")

    received_hash = values.pop("hash")

    # Telegram signs the sorted field list with a secret derived from the bot token.
    data_check_string = "\n".join(f"{key}={values[key]}" for key in sorted(values))
    secret_key = hmac.new(b"WebAppData", bot_token.encode(), hashlib.sha256).digest()
    calculated_hash = hmac.new(secret_key, data_check_string.encode(), hashlib.sha256).hexdigest()
    if not hmac.compare_digest(calculated_hash, received_hash):
        raise ValueError("Telegram Mini App init data signature is invalid")

    auth_date = int(values["auth_date"])
    now = int(time.time())
    if auth_date + max_age_seconds < now:
        raise ValueError("Telegram Mini App init data is expired")

    user_payload = json.loads(values["user"])
    return TelegramMiniAppSession(
        user=TelegramMiniAppUser.model_validate(user_payload),
        auth_date=auth_date,
    )
