use std::time::Duration;

use anyhow::{Result, bail};
use common::config::{
    default_database_url, default_log_level, default_poll_interval_seconds, default_telegram_http_timeout_seconds,
    non_empty,
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

/// Where Telegram delivers updates in webhook mode, and the secret that proves a delivery came from Telegram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookTarget {
    pub url: String,
    pub secret_token: String,
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
    #[serde(default, deserialize_with = "non_empty")]
    pub public_host: Option<String>,
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
    #[serde(default, deserialize_with = "non_empty")]
    pub bot_webhook_secret_token: Option<String>,
    #[serde(default, deserialize_with = "non_empty")]
    pub bot_webhook_url: Option<String>,
    #[serde(default, deserialize_with = "non_empty")]
    pub mini_app_url: Option<String>,
    #[serde(default = "default_mini_app_auth_max_age_seconds")]
    pub mini_app_auth_max_age_seconds: i64,
    #[serde(default = "default_bot_configure_menu_button")]
    pub bot_configure_menu_button: bool,
    /// The feed reads the same variable, so both agree on the interval before the first save.
    #[serde(default = "default_poll_interval_seconds")]
    pub default_poll_interval_seconds: u64,
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

    /// Return the public HTTPS webhook URL and its secret token, which webhook mode cannot run without.
    pub fn webhook_target(&self) -> Result<WebhookTarget> {
        // Without the secret, anyone who finds the webhook URL could send the bot fake updates.
        let Some(secret_token) = self.bot_webhook_secret_token.clone() else {
            bail!("TELEGRAM_NEWS_BOT_WEBHOOK_SECRET_TOKEN is required in webhook mode");
        };
        let url = match (&self.bot_webhook_url, &self.public_host) {
            (Some(url), _) => url.clone(),
            (None, Some(public_host)) => {
                format!("https://{}{}", public_host.trim_end_matches('/'), self.bot_webhook_path)
            }
            (None, None) => {
                bail!("TELEGRAM_NEWS_PUBLIC_HOST or TELEGRAM_NEWS_BOT_WEBHOOK_URL is required in webhook mode")
            }
        };
        Ok(WebhookTarget { url, secret_token })
    }

    /// Return the public Mini App URL for the menu button, when there is one.
    pub fn mini_app_url(&self) -> Option<String> {
        self.mini_app_url.clone().or_else(|| {
            self.public_host.as_ref().map(|public_host| format!("https://{}/app/", public_host.trim_end_matches('/')))
        })
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

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn configs(overrides: Value) -> Configs {
        let mut values = json!({"bot_token": "123:token", "admin_user_id": 1});
        values.as_object_mut().unwrap().extend(overrides.as_object().unwrap().clone());
        serde_json::from_value(values).unwrap()
    }

    #[test]
    fn webhook_target_needs_a_secret_and_prefers_the_explicit_url() {
        let from_host = configs(json!({"public_host": "bot.example/", "bot_webhook_secret_token": "secret"}));
        let explicit = configs(json!({
            "public_host": "bot.example",
            "bot_webhook_url": "https://hook.example/in",
            "bot_webhook_secret_token": "secret",
        }));
        // An empty value counts as unset, so it cannot pass for a secret.
        let empty_secret = configs(json!({"public_host": "bot.example", "bot_webhook_secret_token": ""}));
        let no_url = configs(json!({"bot_webhook_secret_token": "secret"}));

        assert_eq!(from_host.webhook_target().unwrap().url, "https://bot.example/telegram/webhook");
        assert_eq!(
            explicit.webhook_target().unwrap(),
            WebhookTarget { url: "https://hook.example/in".to_owned(), secret_token: "secret".to_owned() }
        );
        assert!(empty_secret.webhook_target().is_err());
        assert!(no_url.webhook_target().is_err());
    }

    #[test]
    fn mini_app_url_prefers_the_explicit_url_over_the_public_host() {
        let explicit = configs(json!({"public_host": "bot.example", "mini_app_url": "https://tunnel.example/app/"}));

        assert_eq!(explicit.mini_app_url().as_deref(), Some("https://tunnel.example/app/"));
        assert_eq!(
            configs(json!({"public_host": "bot.example/"})).mini_app_url().as_deref(),
            Some("https://bot.example/app/")
        );
        assert_eq!(configs(json!({"mini_app_url": ""})).mini_app_url(), None);
    }
}
