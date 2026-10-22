from typing import Literal

from pydantic_settings import BaseSettings, SettingsConfigDict


class AppConfigs(BaseSettings):
    model_config = SettingsConfigDict(env_file=".env", env_prefix="TELEGRAM_NEWS_", extra="ignore")

    database_url: str = "postgresql+psycopg:///telegram_news_feed_bot"
    bot_token: str = ""
    admin_user_id: str = ""
    log_level: str = "INFO"
    public_host: str = ""
    bot_update_mode: Literal["polling", "webhook"] = "polling"
    bot_delete_webhook_on_polling_start: bool = False
    bot_webhook_listen_host: str = "127.0.0.1"
    bot_webhook_path: str = "/telegram/webhook"
    bot_webhook_port: int = 8000
    bot_webhook_secret_token: str = ""
    bot_webhook_url: str = ""
    mini_app_url: str = ""
    mini_app_auth_max_age_seconds: int = 86_400
    bot_configure_menu_button: bool = True
    # The Rust feed reads the same variable, so both agree on the interval before the first save.
    default_poll_interval_seconds: int = 300
    telegram_http_timeout_seconds: float = 20.0
    telegram_get_updates_timeout_seconds: int = 50
    telegram_get_updates_retry_seconds: int = 2
    # The Rust recapper reads the same variable, so both agree on the interval before the first save.
    default_recap_interval_seconds: int = 21_600


def load_configs() -> AppConfigs:
    """
    Load application configuration from environment variables and local files.
    """
    return AppConfigs()
