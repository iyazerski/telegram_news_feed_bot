use std::time::Duration;

use reqwest::RequestBuilder;
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use serde_json::{Value, json};

// reqwest limits whole requests, uploads included, so media of up to 50 MB gets more time than a text message.
const MEDIA_UPLOAD_TIMEOUT: Duration = Duration::from_secs(300);

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

/// One file uploaded through Bot API multipart form data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramUpload {
    pub field_name: String,
    pub filename: String,
    pub content: Vec<u8>,
    pub content_type: String,
}

impl TelegramUpload {
    /// Return whether Telegram should receive the file as a video rather than a photo.
    pub fn is_video(&self) -> bool {
        self.content_type.starts_with("video/")
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
    /// Create a client whose requests give up after `timeout`, or a longer fixed time for media uploads.
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

    /// Send an uploaded photo with an HTML caption to a Telegram chat.
    pub async fn send_photo(&self, chat_id: &str, photo: TelegramUpload, caption: &str) -> Result<(), TelegramError> {
        self.send_media("sendPhoto", "photo", chat_id, photo, caption).await
    }

    /// Send an uploaded video with an HTML caption to a Telegram chat.
    pub async fn send_video(&self, chat_id: &str, video: TelegramUpload, caption: &str) -> Result<(), TelegramError> {
        self.send_media("sendVideo", "video", chat_id, video, caption).await
    }

    /// Send uploaded photos and videos as one album, captioned with HTML, to a Telegram chat.
    pub async fn send_media_group(
        &self,
        chat_id: &str,
        media: Vec<TelegramUpload>,
        caption: &str,
    ) -> Result<(), TelegramError> {
        let method = "sendMediaGroup";
        let mut items = Vec::with_capacity(media.len());
        let mut form = Form::new().text("chat_id", chat_id.to_owned());
        for (index, upload) in media.into_iter().enumerate() {
            let media_type = if upload.is_video() { "video" } else { "photo" };
            let mut item = json!({"type": media_type, "media": format!("attach://{}", upload.field_name)});
            // Telegram shows the first item's caption as the album caption.
            if index == 0 {
                item["caption"] = caption.into();
                item["parse_mode"] = "HTML".into();
            }
            items.push(item);
            form = form.part(upload.field_name.clone(), upload_part(method, upload)?);
        }
        let form = form.text("media", Value::Array(items).to_string());
        self.send(method, self.request(method).timeout(MEDIA_UPLOAD_TIMEOUT).multipart(form)).await
    }

    /// Upload one file in `field` of a captioned media method.
    async fn send_media(
        &self,
        method: &str,
        field: &'static str,
        chat_id: &str,
        upload: TelegramUpload,
        caption: &str,
    ) -> Result<(), TelegramError> {
        let form = Form::new()
            .text("chat_id", chat_id.to_owned())
            .text("caption", caption.to_owned())
            .text("parse_mode", "HTML")
            .part(field, upload_part(method, upload)?);
        self.send(method, self.request(method).timeout(MEDIA_UPLOAD_TIMEOUT).multipart(form)).await
    }

    /// Call a Bot API method with a JSON payload.
    async fn post(&self, method: &str, payload: Value) -> Result<(), TelegramError> {
        self.send(method, self.request(method).json(&payload)).await
    }

    fn request(&self, method: &str) -> RequestBuilder {
        self.http.post(format!("{}/{method}", self.base_url))
    }

    /// Send a Bot API request and turn a refused or malformed answer into an error.
    async fn send(&self, method: &str, request: RequestBuilder) -> Result<(), TelegramError> {
        let http_error = |message: String| TelegramError::Http { method: method.to_owned(), message };
        let response = request
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

/// Turn an upload into a multipart file part.
fn upload_part(method: &str, upload: TelegramUpload) -> Result<Part, TelegramError> {
    Part::bytes(upload.content).file_name(upload.filename).mime_str(&upload.content_type).map_err(|error| {
        TelegramError::Http { method: method.to_owned(), message: format!("invalid media type: {error}") }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{CannedResponse, serve};

    fn api(base_url: String) -> TelegramBotApi {
        TelegramBotApi { base_url, http: reqwest::Client::new() }
    }

    fn upload(index: usize, content_type: &str) -> TelegramUpload {
        TelegramUpload {
            field_name: format!("media{index}"),
            filename: format!("telegram-media-{index}"),
            content: b"data".to_vec(),
            content_type: content_type.to_owned(),
        }
    }

    #[tokio::test]
    async fn malformed_answers_are_retryable_and_refusals_keep_their_code() {
        // A proxy error page is a transport failure, while a Telegram refusal keeps its API error code.
        let (url, server) = serve(vec![
            CannedResponse::new(502, "text/plain", "bad gateway"),
            CannedResponse::new(
                429,
                "application/json",
                r#"{"ok": false, "description": "retry later", "error_code": 429}"#,
            ),
        ])
        .await;
        let api = api(url);

        let proxy_error = api.send_text_message("1", "hello").await.unwrap_err();
        let refusal = api.send_text_message("1", "hello").await.unwrap_err();

        assert!(matches!(proxy_error, TelegramError::Http { .. }) && proxy_error.is_retryable());
        assert!(matches!(refusal, TelegramError::Api { error_code: 429, .. }) && refusal.is_retryable());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn media_group_attaches_every_file_and_captions_the_first() {
        let (url, server) =
            serve(vec![CannedResponse::new(200, "application/json", r#"{"ok": true, "result": []}"#)]).await;

        api(url)
            .send_media_group("@dest", vec![upload(0, "image/jpeg"), upload(1, "video/mp4")], "<b>Hi</b>")
            .await
            .unwrap();

        let requests = server.await.unwrap();
        let request = &requests[0];
        assert!(request.starts_with("POST /sendMediaGroup "));
        assert!(request.contains(r#"name="media0"; filename="telegram-media-0""#));
        assert!(request.contains(r#"name="media1"; filename="telegram-media-1""#));
        // Key order depends on serde_json features, so compare the album description as JSON.
        let (_, media_part) = request.split_once("name=\"media\"\r\n\r\n").unwrap();
        let (media_json, _) = media_part.split_once("\r\n--").unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(media_json).unwrap(),
            json!([
                {"type": "photo", "media": "attach://media0", "caption": "<b>Hi</b>", "parse_mode": "HTML"},
                {"type": "video", "media": "attach://media1"},
            ])
        );
    }
}
