use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

/// A failed Telegram Bot API call.
#[derive(Debug, thiserror::Error)]
pub enum TelegramError {
    /// Telegram answered and refused the request.
    #[error("{method} failed: {description}")]
    Api { method: String, description: String, error_code: u16 },
    /// The request did not get a well-formed Telegram answer.
    #[error("{method} failed: {message}")]
    Http { method: String, message: String },
}

impl TelegramError {
    /// Return whether the request failed for a transient reason and can be retried soon.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Api { error_code, .. } => *error_code == 429 || *error_code >= 500,
            Self::Http { .. } => true,
        }
    }
}

#[derive(Deserialize)]
struct Envelope {
    ok: bool,
    description: Option<String>,
    error_code: Option<u16>,
}

/// Telegram Bot API client for one bot token.
pub struct TelegramBotApi {
    base_url: String,
    http: reqwest::Client,
}

impl TelegramBotApi {
    /// Create a client whose requests give up after `timeout`.
    pub fn new(token: &str, timeout: Duration) -> anyhow::Result<Self> {
        Ok(Self {
            base_url: format!("https://api.telegram.org/bot{token}"),
            http: reqwest::Client::builder().timeout(timeout).build()?,
        })
    }

    /// Send a plain text message to a Telegram chat.
    pub async fn send_text_message(&self, chat_id: &str, text: &str) -> Result<(), TelegramError> {
        self.post("sendMessage", json!({"chat_id": chat_id, "text": text})).await
    }

    /// Send an HTML text message to a Telegram chat.
    pub async fn send_html_message(
        &self,
        chat_id: &str,
        text: &str,
        disable_web_page_preview: bool,
    ) -> Result<(), TelegramError> {
        let payload = json!({
            "chat_id": chat_id,
            "text": text,
            "parse_mode": "HTML",
            "disable_web_page_preview": disable_web_page_preview,
        });
        self.post("sendMessage", payload).await
    }

    /// Call a Bot API method and turn a refused or malformed answer into an error.
    async fn post(&self, method: &str, payload: Value) -> Result<(), TelegramError> {
        let http_error = |message: String| TelegramError::Http { method: method.to_owned(), message };
        let response = self
            .http
            .post(format!("{}/{method}", self.base_url))
            .json(&payload)
            .send()
            .await
            // Telegram errors name the bot URL, so drop it to keep the token out of logs.
            .map_err(|error| http_error(error.without_url().to_string()))?;
        let envelope: Envelope =
            response.json().await.map_err(|error| http_error(format!("invalid response: {}", error.without_url())))?;
        if envelope.ok {
            return Ok(());
        }
        match (envelope.description, envelope.error_code) {
            (Some(description), Some(error_code)) => {
                Err(TelegramError::Api { method: method.to_owned(), description, error_code })
            }
            _ => Err(http_error("invalid error response".to_owned())),
        }
    }
}
