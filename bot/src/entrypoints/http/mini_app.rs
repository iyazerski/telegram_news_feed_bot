use std::sync::Arc;

use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use common::settings::{RecapSchedule, RecapSettings};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::error;

use crate::entrypoints::http::auth::{InitDataError, init_data_secret_key, validate_init_data};
use crate::use_cases::ManageError;
use crate::use_cases::manage_feed::{ChannelPage, FeedService, FeedStore};
use crate::use_cases::manage_recaps::{RecapService, RecapStore};

const INIT_DATA_HEADER: &str = "x-telegram-init-data";
const MAX_CURSOR_CHARS: usize = 120;
const MAX_TOPICS: usize = 10;
const MAX_TOPIC_CHARS: usize = 80;
const MAX_LANGUAGE_CHARS: usize = 40;

/// Who may use the Mini App API: the admin, proven by init data that the bot token signed.
pub struct MiniAppAuth {
    secret_key: [u8; 32],
    admin_user_id: i64,
    max_age_seconds: i64,
}

impl MiniAppAuth {
    /// Accept init data that the bot signed for the admin at most `max_age_seconds` ago.
    pub fn new(bot_token: &str, admin_user_id: i64, max_age_seconds: i64) -> Self {
        Self { secret_key: init_data_secret_key(bot_token), admin_user_id, max_age_seconds }
    }
}

/// The Mini App API's use cases and access rule.
pub struct MiniApp<S> {
    feed: FeedService<S>,
    recaps: RecapService<S>,
    auth: MiniAppAuth,
}

impl<S: FeedStore + RecapStore + 'static> MiniApp<S> {
    pub fn new(feed: FeedService<S>, recaps: RecapService<S>, auth: MiniAppAuth) -> Self {
        Self { feed, recaps, auth }
    }

    /// Return the `/api` routes, which only the admin may call.
    pub fn into_router(self) -> Router {
        Router::new()
            .route("/api/state", get(get_state::<S>))
            .route("/api/channels", get(get_channels::<S>).post(add_channel::<S>))
            .route("/api/channels/{username}", delete(remove_channel::<S>))
            .route("/api/settings/poll-interval", patch(update_poll_interval::<S>))
            .route("/api/recap", put(update_recap::<S>))
            .route("/api/recap/send-now", post(send_recap_now::<S>))
            .with_state(Arc::new(self))
    }
}

/// A failed API request, answered as `{"detail": "..."}`, which the Mini App shows to the user.
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    detail: String,
}

impl ApiError {
    fn new(status: StatusCode, detail: impl Into<String>) -> Self {
        Self { status, detail: detail.into() }
    }

    fn invalid(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, detail)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({"detail": self.detail}))).into_response()
    }
}

impl From<ManageError> for ApiError {
    fn from(manage_error: ManageError) -> Self {
        match manage_error {
            ManageError::Rejected(detail) => Self::new(StatusCode::BAD_REQUEST, detail),
            ManageError::Failed(failure) => failure.into(),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(failure: anyhow::Error) -> Self {
        error!("Mini App request failed: {failure:#}");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong. Please try again.")
    }
}

impl From<InitDataError> for ApiError {
    fn from(init_data_error: InitDataError) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, init_data_error.to_string())
    }
}

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        Self::new(rejection.status(), rejection.body_text())
    }
}

impl From<QueryRejection> for ApiError {
    fn from(rejection: QueryRejection) -> Self {
        Self::new(rejection.status(), rejection.body_text())
    }
}

/// Proof that the configured admin sent the request.
struct Admin;

impl<S: Send + Sync> FromRequestParts<Arc<MiniApp<S>>> for Admin {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, app: &Arc<MiniApp<S>>) -> Result<Self, ApiError> {
        let init_data = parts.headers.get(INIT_DATA_HEADER).and_then(|value| value.to_str().ok());
        let auth = &app.auth;
        let user_id = validate_init_data(
            init_data.ok_or(InitDataError::Missing)?,
            &auth.secret_key,
            auth.max_age_seconds,
            Utc::now(),
        )?;
        if user_id != auth.admin_user_id {
            return Err(ApiError::new(StatusCode::FORBIDDEN, "This bot is restricted to its configured admin."));
        }
        Ok(Self)
    }
}

#[derive(Serialize)]
struct ChannelResponse {
    username: String,
    url: String,
}

impl ChannelResponse {
    fn new(username: String) -> Self {
        let url = format!("https://t.me/{username}");
        Self { username, url }
    }
}

#[derive(Serialize)]
struct ChannelPageResponse {
    channels: Vec<ChannelResponse>,
    next_cursor: Option<String>,
}

impl From<ChannelPage> for ChannelPageResponse {
    fn from(page: ChannelPage) -> Self {
        Self { channels: page.usernames.into_iter().map(ChannelResponse::new).collect(), next_cursor: page.next_cursor }
    }
}

#[derive(Serialize)]
struct SettingsResponse {
    poll_interval_seconds: u64,
}

#[derive(Serialize)]
struct RecapResponse {
    topics: Vec<String>,
    language: String,
    interval_seconds: i64,
    enabled: bool,
    next_run_at: Option<DateTime<Utc>>,
}

impl From<RecapSchedule> for RecapResponse {
    fn from(RecapSchedule { settings, next_run_at }: RecapSchedule) -> Self {
        Self {
            topics: settings.topics,
            language: settings.language,
            interval_seconds: settings.interval_seconds,
            enabled: settings.enabled,
            next_run_at,
        }
    }
}

#[derive(Serialize)]
struct AppStateResponse {
    poll_interval_seconds: u64,
    #[serde(flatten)]
    page: ChannelPageResponse,
    recap: RecapResponse,
}

#[derive(Deserialize)]
struct ChannelsQuery {
    #[serde(default)]
    after: String,
}

#[derive(Deserialize)]
struct AddChannelRequest {
    username_or_url: String,
}

/// The poll intervals the Mini App offers.
#[derive(Deserialize)]
enum PollInterval {
    #[serde(rename = "1m")]
    OneMinute,
    #[serde(rename = "5m")]
    FiveMinutes,
    #[serde(rename = "15m")]
    FifteenMinutes,
    #[serde(rename = "1h")]
    OneHour,
}

impl PollInterval {
    fn seconds(&self) -> u64 {
        match self {
            Self::OneMinute => 60,
            Self::FiveMinutes => 300,
            Self::FifteenMinutes => 900,
            Self::OneHour => 3_600,
        }
    }
}

#[derive(Deserialize)]
struct PollIntervalRequest {
    interval: PollInterval,
}

/// The recap intervals the Mini App offers.
#[derive(Deserialize)]
enum RecapInterval {
    #[serde(rename = "3h")]
    ThreeHours,
    #[serde(rename = "6h")]
    SixHours,
    #[serde(rename = "12h")]
    TwelveHours,
    #[serde(rename = "24h")]
    OneDay,
}

impl RecapInterval {
    fn seconds(&self) -> i64 {
        match self {
            Self::ThreeHours => 10_800,
            Self::SixHours => 21_600,
            Self::TwelveHours => 43_200,
            Self::OneDay => 86_400,
        }
    }
}

#[derive(Deserialize)]
struct RecapSettingsRequest {
    topics: Vec<String>,
    language: String,
    interval: RecapInterval,
    enabled: bool,
}

impl RecapSettingsRequest {
    /// Return the trimmed settings, with repeated topics once, or why they are invalid.
    fn into_settings(self) -> Result<RecapSettings, ApiError> {
        if self.topics.len() > MAX_TOPICS {
            return Err(ApiError::invalid(format!("Use at most {MAX_TOPICS} topics.")));
        }
        let mut topics: Vec<String> = Vec::with_capacity(self.topics.len());
        for topic in &self.topics {
            let topic = require_length(topic, MAX_TOPIC_CHARS, "Each topic")?;
            if !topics.contains(&topic) {
                topics.push(topic);
            }
        }
        Ok(RecapSettings {
            topics,
            language: require_length(&self.language, MAX_LANGUAGE_CHARS, "The language")?,
            interval_seconds: self.interval.seconds(),
            enabled: self.enabled,
        })
    }
}

/// Return `value` trimmed, or an error when that leaves it empty or longer than `max_chars`.
fn require_length(value: &str, max_chars: usize, name: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max_chars {
        return Err(ApiError::invalid(format!("{name} must have 1 to {max_chars} characters.")));
    }
    Ok(value.to_owned())
}

async fn get_state<S: FeedStore + RecapStore>(
    State(app): State<Arc<MiniApp<S>>>,
    _admin: Admin,
) -> Result<Json<AppStateResponse>, ApiError> {
    // The pool has room for all three queries at once.
    let (poll_interval_seconds, page, recap) =
        tokio::try_join!(app.feed.poll_interval_seconds(), app.feed.channel_page(""), app.recaps.schedule())?;
    Ok(Json(AppStateResponse { poll_interval_seconds, page: page.into(), recap: recap.into() }))
}

async fn get_channels<S: FeedStore + RecapStore>(
    State(app): State<Arc<MiniApp<S>>>,
    _admin: Admin,
    query: Result<Query<ChannelsQuery>, QueryRejection>,
) -> Result<Json<ChannelPageResponse>, ApiError> {
    let Query(query) = query?;
    if query.after.chars().count() > MAX_CURSOR_CHARS {
        return Err(ApiError::invalid(format!("The cursor must have at most {MAX_CURSOR_CHARS} characters.")));
    }
    Ok(Json(app.feed.channel_page(&query.after).await?.into()))
}

async fn add_channel<S: FeedStore + RecapStore>(
    State(app): State<Arc<MiniApp<S>>>,
    _admin: Admin,
    request: Result<Json<AddChannelRequest>, JsonRejection>,
) -> Result<Json<ChannelResponse>, ApiError> {
    let Json(request) = request?;
    Ok(Json(ChannelResponse::new(app.feed.add_channel(&request.username_or_url).await?)))
}

async fn remove_channel<S: FeedStore + RecapStore>(
    State(app): State<Arc<MiniApp<S>>>,
    _admin: Admin,
    Path(username): Path<String>,
) -> Result<StatusCode, ApiError> {
    app.feed.remove_channel(&username).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_poll_interval<S: FeedStore + RecapStore>(
    State(app): State<Arc<MiniApp<S>>>,
    _admin: Admin,
    request: Result<Json<PollIntervalRequest>, JsonRejection>,
) -> Result<Json<SettingsResponse>, ApiError> {
    let Json(request) = request?;
    let seconds = request.interval.seconds();
    app.feed.set_poll_interval_seconds(seconds).await?;
    Ok(Json(SettingsResponse { poll_interval_seconds: seconds }))
}

async fn update_recap<S: FeedStore + RecapStore>(
    State(app): State<Arc<MiniApp<S>>>,
    _admin: Admin,
    request: Result<Json<RecapSettingsRequest>, JsonRejection>,
) -> Result<Json<RecapResponse>, ApiError> {
    let Json(request) = request?;
    Ok(Json(app.recaps.save(request.into_settings()?, Utc::now()).await?.into()))
}

async fn send_recap_now<S: FeedStore + RecapStore>(
    State(app): State<Arc<MiniApp<S>>>,
    _admin: Admin,
) -> Result<Json<RecapResponse>, ApiError> {
    Ok(Json(app.recaps.request_now(Utc::now()).await?.into()))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Method, Request};
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;
    use crate::entrypoints::http::auth::tests::{BOT_TOKEN, signed_init_data};
    use crate::testing::FakeStore;
    use crate::use_cases::manage_feed::CHANNEL_PAGE_SIZE;

    const ADMIN_ID: i64 = 123;

    /// The Mini App API over `store`, as the admin's Mini App calls it.
    struct Client {
        router: Router,
        init_data: String,
    }

    impl Client {
        fn new(store: &Arc<FakeStore>) -> Self {
            let app = MiniApp::new(
                FeedService::new(Arc::clone(store), 300),
                RecapService::new(Arc::clone(store), 21_600),
                MiniAppAuth::new(BOT_TOKEN, ADMIN_ID, 3_600),
            );
            Self { router: app.into_router(), init_data: signed_init_data(BOT_TOKEN, ADMIN_ID, Utc::now().timestamp()) }
        }

        /// Send a request and return its status and JSON body, which is null when there is none.
        async fn send(&self, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
            let request = Request::builder()
                .method(method)
                .uri(uri)
                .header(INIT_DATA_HEADER, &self.init_data)
                .header("content-type", "application/json");
            let body = body.map_or_else(Body::empty, |body| Body::from(body.to_string()));
            let response = self.router.clone().oneshot(request.body(body).unwrap()).await.unwrap();
            let status = response.status();
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
            (status, if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap() })
        }
    }

    fn usernames(page: &Value) -> Vec<&str> {
        page["channels"].as_array().unwrap().iter().map(|channel| channel["username"].as_str().unwrap()).collect()
    }

    #[tokio::test]
    async fn channel_pages_list_active_channels_after_the_cursor() {
        let store = Arc::new(FakeStore::default());
        let names: Vec<String> = (0..CHANNEL_PAGE_SIZE + 2).map(|index| format!("channel{index:04}")).collect();
        for name in &names {
            store.add_channel(name, true);
        }
        store.add_channel("channel0000inactive", false);
        let client = Client::new(&store);

        let (status, first) = client.send(Method::GET, "/api/state", None).await;
        let cursor = first["next_cursor"].as_str().unwrap();
        let (_, second) = client.send(Method::GET, &format!("/api/channels?after={cursor}"), None).await;
        let (too_long, _) = client.send(Method::GET, &format!("/api/channels?after={}", "a".repeat(121)), None).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(usernames(&first), names[..CHANNEL_PAGE_SIZE]);
        assert_eq!(cursor, names[CHANNEL_PAGE_SIZE - 1]);
        assert_eq!(usernames(&second), names[CHANNEL_PAGE_SIZE..]);
        assert_eq!(second["next_cursor"], Value::Null);
        assert_eq!(too_long, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn mutations_return_only_what_changed() {
        let store = Arc::new(FakeStore::default());
        let client = Client::new(&store);
        let channel = json!({"username": "example", "url": "https://t.me/example"});

        let added = client.send(Method::POST, "/api/channels", Some(json!({"username_or_url": "@Example"}))).await;
        let interval =
            client.send(Method::PATCH, "/api/settings/poll-interval", Some(json!({"interval": "15m"}))).await;
        let removed = client.send(Method::DELETE, "/api/channels/example", None).await;
        let (_, state) = client.send(Method::GET, "/api/state", None).await;
        let invalid = client.send(Method::POST, "/api/channels", Some(json!({"username_or_url": "t.me/a/1"}))).await;

        assert_eq!(added, (StatusCode::OK, channel));
        assert_eq!(interval, (StatusCode::OK, json!({"poll_interval_seconds": 900})));
        assert_eq!(removed, (StatusCode::NO_CONTENT, Value::Null));
        assert_eq!(
            (&state["channels"], &state["next_cursor"], &state["poll_interval_seconds"]),
            (&json!([]), &Value::Null, &json!(900))
        );
        assert_eq!(
            invalid,
            (StatusCode::BAD_REQUEST, json!({"detail": "Channel username must not include a message path"}))
        );
    }

    #[tokio::test]
    async fn recap_settings_schedule_recaps_and_validate_input() {
        let store = Arc::new(FakeStore::default());
        let client = Client::new(&store);
        let request =
            json!({"topics": [" AI ", "AI", "Space"], "language": " Russian ", "interval": "12h", "enabled": true});

        let (_, state) = client.send(Method::GET, "/api/state", None).await;
        let (early_status, _) = client.send(Method::POST, "/api/recap/send-now", None).await;
        let (saved_status, saved) = client.send(Method::PUT, "/api/recap", Some(request.clone())).await;
        let (now_status, _) = client.send(Method::POST, "/api/recap/send-now", None).await;
        let (_, turned_off) = client
            .send(
                Method::PUT,
                "/api/recap",
                Some(json!({"topics": ["AI"], "language": "English", "interval": "6h", "enabled": false})),
            )
            .await;

        assert_eq!(
            state["recap"],
            json!({"topics": [], "language": "English", "interval_seconds": 21_600, "enabled": false, "next_run_at": null})
        );
        assert_eq!((early_status, saved_status, now_status), (StatusCode::BAD_REQUEST, StatusCode::OK, StatusCode::OK));
        assert_eq!(
            (&saved["topics"], &saved["language"], &saved["interval_seconds"]),
            (&json!(["AI", "Space"]), &json!("Russian"), &json!(43_200))
        );
        assert!(saved["next_run_at"].is_string());
        assert_eq!(turned_off["next_run_at"], Value::Null);
    }

    #[tokio::test]
    async fn invalid_recap_requests_explain_what_is_wrong() {
        let client = Client::new(&Arc::new(FakeStore::default()));
        let request = json!({"topics": ["AI"], "language": "English", "interval": "6h", "enabled": true});
        let with = |key: &str, value: Value| {
            let mut request = request.clone();
            request[key] = value;
            Some(request)
        };

        for (body, status) in [
            (with("topics", json!([])), StatusCode::BAD_REQUEST),
            (with("interval", json!("2h")), StatusCode::UNPROCESSABLE_ENTITY),
            (with("topics", json!(["  "])), StatusCode::UNPROCESSABLE_ENTITY),
            (with("topics", json!(vec!["AI"; 11])), StatusCode::UNPROCESSABLE_ENTITY),
            (with("language", json!("x".repeat(41))), StatusCode::UNPROCESSABLE_ENTITY),
        ] {
            let (actual, detail) = client.send(Method::PUT, "/api/recap", body).await;
            assert_eq!(actual, status);
            assert!(detail["detail"].is_string(), "{detail}");
        }
    }

    #[tokio::test]
    async fn only_the_admin_with_signed_init_data_may_call_the_api() {
        let store = Arc::new(FakeStore::default());
        let mut client = Client::new(&store);

        client.init_data = signed_init_data(BOT_TOKEN, 999, Utc::now().timestamp());
        let (other_user, _) = client.send(Method::GET, "/api/state", None).await;
        client.init_data = signed_init_data("456:other-token", ADMIN_ID, Utc::now().timestamp());
        let forged = client.send(Method::GET, "/api/state", None).await;

        assert_eq!(other_user, StatusCode::FORBIDDEN);
        assert_eq!(
            forged,
            (StatusCode::UNAUTHORIZED, json!({"detail": "Telegram Mini App init data signature is invalid"}))
        );
    }
}
