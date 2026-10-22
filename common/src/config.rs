use anyhow::{Context, Result};
use serde::de::DeserializeOwned;

/// Load service settings from `TELEGRAM_NEWS_*` variables, after a local `.env` file when there is one.
pub fn load<T: DeserializeOwned>() -> Result<T> {
    // A missing .env is normal outside local development.
    let _ = dotenvy::dotenv();
    envy::prefixed("TELEGRAM_NEWS_").from_env::<T>().context("invalid TELEGRAM_NEWS_* settings")
}

/// Return the database URL the Python services use by default.
pub fn default_database_url() -> String {
    "postgresql+psycopg:///telegram_news_feed_bot".to_owned()
}

/// Return the log level the Python services use by default.
pub fn default_log_level() -> String {
    "INFO".to_owned()
}

/// Return the Telegram Bot API request timeout the Python services use by default.
pub fn default_telegram_http_timeout_seconds() -> f64 {
    20.0
}
