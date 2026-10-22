from pydantic_settings import BaseSettings, SettingsConfigDict


class AppConfigs(BaseSettings):
    model_config = SettingsConfigDict(env_file=".env", env_prefix="TELEGRAM_NEWS_", extra="ignore")

    database_url: str = "postgresql+psycopg:///telegram_news_feed_bot"


def load_configs() -> AppConfigs:
    """
    Load the migration settings from environment variables and a local .env file.
    """
    return AppConfigs()
