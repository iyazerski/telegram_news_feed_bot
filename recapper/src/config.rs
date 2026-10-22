use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// Recapper settings, read from `TELEGRAM_NEWS_*` variables like the Python services' `AppConfigs`.
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
    #[serde(default = "default_recap_interval_seconds")]
    pub default_recap_interval_seconds: i64,
    #[serde(default = "default_recap_check_interval_seconds")]
    pub recap_check_interval_seconds: u64,
    #[serde(default = "default_recap_retry_delay_seconds")]
    pub recap_retry_delay_seconds: i64,
    #[serde(default = "default_recap_story_count")]
    pub recap_story_count: usize,
    #[serde(default = "default_recap_history_size")]
    pub recap_history_size: i64,
    #[serde(default = "default_recap_history_retention_days")]
    pub recap_history_retention_days: i64,
    #[serde(default = "default_recap_timeout_seconds")]
    pub recap_timeout_seconds: u64,
    /// Directory that keeps the ChatGPT sign-in, which the recapper refreshes. Production mounts a volume here.
    #[serde(default = "default_recap_auth_dir")]
    pub recap_auth_dir: String,
    #[serde(default)]
    pub recap_tavily_api_key: String,
}

impl Configs {
    /// Load settings from the environment, after a local `.env` file when there is one.
    pub fn load() -> Result<Self> {
        // A missing .env is normal outside local development.
        let _ = dotenvy::dotenv();
        envy::prefixed("TELEGRAM_NEWS_").from_env::<Self>().context("invalid TELEGRAM_NEWS_* settings")
    }

    /// Fail fast on settings the recap loop cannot run without.
    pub fn require_service_settings(&self) -> Result<()> {
        if self.bot_token.is_empty() {
            bail!("TELEGRAM_NEWS_BOT_TOKEN is required");
        }
        if self.recap_tavily_api_key.is_empty() {
            bail!("TELEGRAM_NEWS_RECAP_TAVILY_API_KEY is required");
        }
        Ok(())
    }

    /// Return the database URL in the form sqlx accepts.
    pub fn postgres_url(&self) -> String {
        // The shared URL names SQLAlchemy's driver, as in `postgresql+psycopg://`, which sqlx does not understand.
        match self.database_url.split_once("://") {
            Some((scheme, rest)) => {
                let scheme = scheme.split_once('+').map_or(scheme, |(base, _)| base);
                format!("{scheme}://{rest}")
            }
            None => self.database_url.clone(),
        }
    }

    /// Return the Telegram Bot API request timeout.
    pub fn telegram_http_timeout(&self) -> Duration {
        Duration::from_secs_f64(self.telegram_http_timeout_seconds)
    }
}

fn default_database_url() -> String {
    "postgresql+psycopg:///telegram_news_feed_bot".to_owned()
}

fn default_log_level() -> String {
    "INFO".to_owned()
}

fn default_telegram_http_timeout_seconds() -> f64 {
    20.0
}

fn default_recap_interval_seconds() -> i64 {
    21_600
}

fn default_recap_check_interval_seconds() -> u64 {
    60
}

fn default_recap_retry_delay_seconds() -> i64 {
    900
}

fn default_recap_story_count() -> usize {
    5
}

fn default_recap_history_size() -> i64 {
    50
}

fn default_recap_history_retention_days() -> i64 {
    30
}

fn default_recap_timeout_seconds() -> u64 {
    600
}

fn default_recap_auth_dir() -> String {
    ".recapper-auth".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_url_drops_the_sqlalchemy_driver() {
        let mut configs: Configs = envy::prefixed("UNSET_PREFIX_").from_iter(Vec::new()).unwrap();
        configs.database_url = "postgresql+psycopg://user:pass@db:5432/news".to_owned();
        assert_eq!(configs.postgres_url(), "postgresql://user:pass@db:5432/news");
        configs.database_url = "postgresql://db/news".to_owned();
        assert_eq!(configs.postgres_url(), "postgresql://db/news");
    }
}
