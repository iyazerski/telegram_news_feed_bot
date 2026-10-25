use std::fmt;
use std::time::Duration;

use reqwest::RequestBuilder;
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use serde::de::{DeserializeOwned, IgnoredAny};
use serde_json::{Value, json};

/// The updates the bot asks Telegram for: commands, and the bot being blocked or removed from a chat.
const ALLOWED_UPDATES: [&str; 2] = ["message", "my_chat_member"];
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

/// Whether Telegram shows a preview of the first link in a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkPreview {
    Show,
    Hide,
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

/// One incoming bot update. The bot asks Telegram only for messages and changes of its own membership.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Update {
    pub update_id: i64,
    pub message: Option<Message>,
    pub my_chat_member: Option<ChatMemberUpdated>,
}

/// A change of the bot's membership in a chat, as when a user blocks or restarts the bot.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChatMemberUpdated {
    pub chat: Chat,
    pub new_chat_member: ChatMember,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChatMember {
    pub status: ChatMemberStatus,
}

/// Whether the bot can still send to a chat. A private chat that blocks the bot reports `Kicked`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatMemberStatus {
    Left,
    Kicked,
    /// Any status that keeps the bot in the chat.
    #[serde(other)]
    Present,
}

impl ChatMemberStatus {
    /// Return whether the bot can no longer send messages to the chat.
    pub fn is_gone(self) -> bool {
        matches!(self, Self::Left | Self::Kicked)
    }
}

/// A message sent to the bot.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Message {
    pub chat: Chat,
    /// The sender, which messages in channels do not have.
    pub from: Option<User>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Chat {
    pub id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct User {
    pub id: i64,
}

#[derive(Deserialize)]
struct Envelope<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
    error_code: Option<u16>,
}

/// Telegram Bot API client for one bot token. Clones share one connection pool.
#[derive(Clone)]
pub struct TelegramBotApi {
    base_url: String,
    http: reqwest::Client,
    timeout: Duration,
}

impl fmt::Debug for TelegramBotApi {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The base URL holds the bot token, so it stays out of debug output.
        formatter.debug_struct("TelegramBotApi").field("timeout", &self.timeout).finish_non_exhaustive()
    }
}

impl TelegramBotApi {
    /// Create a client whose requests give up after `timeout`, or later for long polls and media uploads.
    pub fn new(token: &str, timeout: Duration) -> anyhow::Result<Self> {
        Ok(Self {
            base_url: format!("https://api.telegram.org/bot{token}"),
            http: reqwest::Client::builder().timeout(timeout).build()?,
            timeout,
        })
    }

    /// Wait up to `wait` for updates from `offset` on, which also confirms the updates before it.
    pub async fn get_updates(&self, offset: i64, wait: Duration) -> Result<Vec<Update>, TelegramError> {
        let method = "getUpdates";
        let payload = json!({"offset": offset, "timeout": wait.as_secs(), "allowed_updates": ALLOWED_UPDATES});
        // Telegram holds a long poll open for `wait`, so the request may take that long before its own timeout.
        self.send(method, self.request(method).timeout(wait + self.timeout).json(&payload)).await
    }

    /// Have Telegram deliver updates to `url`, sending `secret_token` in each request's header.
    pub async fn set_webhook(&self, url: &str, secret_token: &str) -> Result<(), TelegramError> {
        let payload = json!({"url": url, "secret_token": secret_token, "allowed_updates": ALLOWED_UPDATES});
        self.post("setWebhook", payload).await
    }

    /// Stop webhook delivery, keeping pending updates, so that long polling can take over.
    pub async fn delete_webhook(&self) -> Result<(), TelegramError> {
        self.post("deleteWebhook", json!({"drop_pending_updates": false})).await
    }

    /// Make the bot's default menu button open the Mini App at `web_app_url`.
    pub async fn set_chat_menu_button(&self, text: &str, web_app_url: &str) -> Result<(), TelegramError> {
        let payload = json!({"menu_button": {"type": "web_app", "text": text, "web_app": {"url": web_app_url}}});
        self.post("setChatMenuButton", payload).await
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
        link_preview: LinkPreview,
    ) -> Result<(), TelegramError> {
        let payload = json!({
            "chat_id": chat_id,
            "text": text,
            "parse_mode": "HTML",
            "disable_web_page_preview": link_preview == LinkPreview::Hide,
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
        self.send_ignoring_result(method, self.request(method).timeout(MEDIA_UPLOAD_TIMEOUT).multipart(form)).await
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
        self.send_ignoring_result(method, self.request(method).timeout(MEDIA_UPLOAD_TIMEOUT).multipart(form)).await
    }

    /// Call a Bot API method with a JSON payload.
    async fn post(&self, method: &str, payload: Value) -> Result<(), TelegramError> {
        self.send_ignoring_result(method, self.request(method).json(&payload)).await
    }

    async fn send_ignoring_result(&self, method: &str, request: RequestBuilder) -> Result<(), TelegramError> {
        self.send::<IgnoredAny>(method, request).await.map(|_| ())
    }

    fn request(&self, method: &str) -> RequestBuilder {
        self.http.post(format!("{}/{method}", self.base_url))
    }

    /// Send a Bot API request, return its result, and turn a refused or malformed answer into an error.
    async fn send<T: DeserializeOwned>(&self, method: &str, request: RequestBuilder) -> Result<T, TelegramError> {
        let http_error = |message: String| TelegramError::Http { method: method.to_owned(), message };
        let response = request
            .send()
            .await
            // Telegram errors name the bot URL, so drop it to keep the token out of logs.
            .map_err(|error| http_error(error.without_url().to_string()))?;
        let envelope: Envelope<T> =
            response.json().await.map_err(|error| http_error(format!("invalid response: {}", error.without_url())))?;
        if envelope.ok {
            return envelope.result.ok_or_else(|| http_error("missing result".to_owned()));
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
        TelegramBotApi { base_url, http: reqwest::Client::new(), timeout: Duration::from_secs(5) }
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
    async fn get_updates_asks_for_messages_and_membership_changes_after_the_offset_and_reads_them() {
        let membership = |status: &str| {
            format!(
                r#"{{"chat": {{"id": 42, "type": "private"}}, "from": {{"id": 42, "is_bot": false, "first_name": "A"}},
                    "date": 1, "old_chat_member": {{"status": "member", "user": {{"id": 1, "is_bot": true}}}},
                    "new_chat_member": {{"status": "{status}", "user": {{"id": 1, "is_bot": true}}}}}}"#
            )
        };
        let body = format!(
            r#"{{"ok": true, "result": [
                {{"update_id": 7, "message": {{"message_id": 1, "chat": {{"id": 42, "type": "private"}},
                    "from": {{"id": 5, "is_bot": false, "first_name": "A"}}, "text": "/start"}}}},
                {{"update_id": 8, "edited_message": {{}}}},
                {{"update_id": 9, "my_chat_member": {}}},
                {{"update_id": 10, "my_chat_member": {}}}
            ]}}"#,
            membership("kicked"),
            membership("administrator"),
        );
        let (url, server) = serve(vec![CannedResponse::new(200, "application/json", body)]).await;

        let updates = api(url).get_updates(7, Duration::from_secs(1)).await.unwrap();

        let start = Message { chat: Chat { id: 42 }, from: Some(User { id: 5 }), text: Some("/start".to_owned()) };
        let membership = |update_id, status| Update {
            update_id,
            message: None,
            my_chat_member: Some(ChatMemberUpdated { chat: Chat { id: 42 }, new_chat_member: ChatMember { status } }),
        };
        assert_eq!(
            updates,
            [
                Update { update_id: 7, message: Some(start), my_chat_member: None },
                Update { update_id: 8, message: None, my_chat_member: None },
                membership(9, ChatMemberStatus::Kicked),
                membership(10, ChatMemberStatus::Present),
            ]
        );
        let requests = server.await.unwrap();
        let (_, payload) = requests[0].split_once("\r\n\r\n").unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(payload).unwrap(),
            json!({"offset": 7, "timeout": 1, "allowed_updates": ["message", "my_chat_member"]})
        );
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
