use std::time::Duration;

use anyhow::{Result, bail};
use common::config::{
    default_database_url, default_log_level, default_poll_interval_seconds, default_recap_interval_seconds,
    default_telegram_http_timeout_seconds,
};
use serde::Deserialize;

/// How the bot receives Telegram updates. Telegram allows only one of them per bot token at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateMode {
    /// Long polling, for local development without a public URL.
    #[default]
    Polling,
    /// An HTTPS webhook behind the Kubernetes ingress.
    Webhook,
}

/// Bot settings, read from `TELEGRAM_NEWS_*` variables.
#[derive(Debug, Deserialize)]
pub struct Configs {
    #[serde(default = "default_database_url")]
    pub database_url: String,
    pub bot_token: String,
    /// The only Telegram user the bot and the Mini App serve.
    pub admin_user_id: i64,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub public_host: String,
    #[serde(default)]
    pub bot_update_mode: UpdateMode,
    #[serde(default)]
    pub bot_delete_webhook_on_polling_start: bool,
    #[serde(default = "default_bot_webhook_listen_host")]
    pub bot_webhook_listen_host: String,
    #[serde(default = "default_bot_webhook_path")]
    pub bot_webhook_path: String,
    #[serde(default = "default_bot_webhook_port")]
    pub bot_webhook_port: u16,
    #[serde(default)]
    pub bot_webhook_secret_token: String,
    #[serde(default)]
    pub bot_webhook_url: String,
    #[serde(default)]
    pub mini_app_url: String,
    #[serde(default = "default_mini_app_auth_max_age_seconds")]
    pub mini_app_auth_max_age_seconds: i64,
    #[serde(default = "default_bot_configure_menu_button")]
    pub bot_configure_menu_button: bool,
    /// The feed reads the same variable, so both agree on the interval before the first save.
    #[serde(default = "default_poll_interval_seconds")]
    pub default_poll_interval_seconds: u64,
    /// The recapper reads the same variable, so both agree on the interval before the first save.
    #[serde(default = "default_recap_interval_seconds")]
    pub default_recap_interval_seconds: i64,
    #[serde(default = "default_telegram_http_timeout_seconds")]
    pub telegram_http_timeout_seconds: f64,
    #[serde(default = "default_telegram_get_updates_timeout_seconds")]
    pub telegram_get_updates_timeout_seconds: u64,
    #[serde(default = "default_telegram_get_updates_retry_seconds")]
    pub telegram_get_updates_retry_seconds: u64,
}

impl Configs {
    /// Load settings from the environment, after a local `.env` file when there is one.
    pub fn load() -> Result<Self> {
        common::config::load()
    }

    /// Fail fast on settings the bot cannot run without.
    pub fn require_service_settings(&self) -> Result<()> {
        if self.bot_update_mode == UpdateMode::Webhook {
            // Without the secret, anyone who finds the webhook URL could send the bot fake updates.
            if self.bot_webhook_secret_token.is_empty() {
                bail!("TELEGRAM_NEWS_BOT_WEBHOOK_SECRET_TOKEN is required in webhook mode");
            }
            self.webhook_url()?;
        }
        Ok(())
    }

    /// Return the public HTTPS webhook URL to register with Telegram.
    pub fn webhook_url(&self) -> Result<String> {
        if !self.bot_webhook_url.is_empty() {
            return Ok(self.bot_webhook_url.clone());
        }
        if !self.public_host.is_empty() {
            return Ok(format!("https://{}{}", self.public_host.trim_end_matches('/'), self.bot_webhook_path));
        }
        bail!("TELEGRAM_NEWS_PUBLIC_HOST or TELEGRAM_NEWS_BOT_WEBHOOK_URL is required in webhook mode")
    }

    /// Return the public Mini App URL for the menu button, when there is one.
    pub fn mini_app_url(&self) -> Option<String> {
        if !self.mini_app_url.is_empty() {
            return Some(self.mini_app_url.clone());
        }
        (!self.public_host.is_empty()).then(|| format!("https://{}/app/", self.public_host.trim_end_matches('/')))
    }

    /// Return the Telegram Bot API request timeout.
    pub fn telegram_http_timeout(&self) -> Duration {
        Duration::from_secs_f64(self.telegram_http_timeout_seconds)
    }
}

fn default_bot_webhook_listen_host() -> String {
    "127.0.0.1".to_owned()
}

fn default_bot_webhook_path() -> String {
    "/telegram/webhook".to_owned()
}

fn default_bot_webhook_port() -> u16 {
    8000
}

fn default_mini_app_auth_max_age_seconds() -> i64 {
    86_400
}

fn default_bot_configure_menu_button() -> bool {
    true
}

fn default_telegram_get_updates_timeout_seconds() -> u64 {
    50
}

fn default_telegram_get_updates_retry_seconds() -> u64 {
    2
}
