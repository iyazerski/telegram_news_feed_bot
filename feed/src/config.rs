use std::time::Duration;

use anyhow::{Result, bail};
use common::config::{
    default_database_url, default_log_level, default_poll_interval_seconds, default_telegram_http_timeout_seconds,
};
use serde::Deserialize;

/// Feed settings, read from `TELEGRAM_NEWS_*` variables.
#[derive(Debug, Deserialize)]
pub struct Configs {
    #[serde(default = "default_database_url")]
    pub database_url: String,
    #[serde(default)]
    pub bot_token: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_telegram_http_timeout_seconds")]
    pub telegram_http_timeout_seconds: f64,
    #[serde(default = "default_preview_http_timeout_seconds")]
    pub preview_http_timeout_seconds: f64,
    /// The most media one post may download and upload, which the feed holds in memory while it sends the post.
    #[serde(default = "default_post_media_max_bytes")]
    pub post_media_max_bytes: u64,
    /// The bot reads the same variable, so both agree on the interval before the first save.
    #[serde(default = "default_poll_interval_seconds")]
    pub default_poll_interval_seconds: u64,
}

impl Configs {
    /// Load settings from the environment, after a local `.env` file when there is one.
    pub fn load() -> Result<Self> {
        common::config::load()
    }

    /// Fail fast on settings the feed cannot run without.
    pub fn require_service_settings(&self) -> Result<()> {
        if self.bot_token.is_empty() {
            bail!("TELEGRAM_NEWS_BOT_TOKEN is required");
        }
        Ok(())
    }

    /// Return the Telegram Bot API and media download timeout.
    pub fn telegram_http_timeout(&self) -> Duration {
        Duration::from_secs_f64(self.telegram_http_timeout_seconds)
    }

    /// Return the channel web preview request timeout.
    pub fn preview_http_timeout(&self) -> Duration {
        Duration::from_secs_f64(self.preview_http_timeout_seconds)
    }
}

fn default_preview_http_timeout_seconds() -> f64 {
    10.0
}

// Telegram bots can upload videos of up to 50 MB.
fn default_post_media_max_bytes() -> u64 {
    50_000_000
}
