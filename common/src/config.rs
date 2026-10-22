use anyhow::{Context, Result};
use serde::de::DeserializeOwned;

/// Load service settings from `TELEGRAM_NEWS_*` variables, after a local `.env` file when there is one.
pub fn load<T: DeserializeOwned>() -> Result<T> {
    // A missing .env is normal outside local development.
    let _ = dotenvy::dotenv();
    envy::prefixed("TELEGRAM_NEWS_").from_env::<T>().context("invalid TELEGRAM_NEWS_* settings")
}

/// Return the local database URL every service uses by default.
pub fn default_database_url() -> String {
    "postgresql:///telegram_news_feed_bot".to_owned()
}

/// Return the log level every service uses by default.
pub fn default_log_level() -> String {
    "INFO".to_owned()
}

/// Return the Telegram Bot API request timeout every service uses by default.
pub fn default_telegram_http_timeout_seconds() -> f64 {
    20.0
}

/// Return the poll interval that the bot shows and the feed uses before the first save.
pub fn default_poll_interval_seconds() -> u64 {
    300
}

/// Return the recap interval that the bot shows and the recapper uses before the first save.
pub fn default_recap_interval_seconds() -> i64 {
    21_600
}
