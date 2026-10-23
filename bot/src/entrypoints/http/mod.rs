pub mod auth;
pub mod mini_app;

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect};
use axum::routing::{get, post};
use common::telegram::bot_api::Update;
use subtle::ConstantTimeEq;
use tracing::error;

use crate::entrypoints::commands::{BotMessenger, CommandHandler};
use crate::use_cases::manage_feed::FeedStore;

const WEBHOOK_SECRET_HEADER: &str = "x-telegram-bot-api-secret-token";
// The Mini App is built into the binary, so the image needs no files next to it.
const INDEX_HTML: &str = include_str!("static/index.html");
const APP_JS: &str = include_str!("static/app.js");
const STYLES_CSS: &str = include_str!("static/styles.css");

/// Receives updates that Telegram posts to the webhook, which proves itself with the secret token.
pub struct Webhook<S, M> {
    pub path: String,
    pub secret_token: String,
    pub commands: CommandHandler<S, M>,
}

impl<S: FeedStore + 'static, M: BotMessenger + 'static> Webhook<S, M> {
    /// Return the route that receives the updates.
    pub fn into_router(self) -> Router {
        let path = self.path.clone();
        Router::new().route(&path, post(receive_update::<S, M>)).with_state(Arc::new(self))
    }
}

/// Return the app that serves health checks and the Mini App with its API.
pub fn create_app(mini_app: Router) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        // Relative asset paths in the page need the trailing slash.
        .route("/", get(|| async { Redirect::temporary("/app/") }))
        .route("/app", get(|| async { Redirect::temporary("/app/") }))
        .route("/app/", get(|| async { asset("text/html; charset=utf-8", INDEX_HTML) }))
        .route("/app/app.js", get(|| async { asset("text/javascript; charset=utf-8", APP_JS) }))
        .route("/app/styles.css", get(|| async { asset("text/css; charset=utf-8", STYLES_CSS) }))
        .merge(mini_app)
}

/// Answer with a built-in Mini App file, which browsers check again on each load so a deploy shows up at once.
fn asset(content_type: &'static str, body: &'static str) -> impl IntoResponse {
    ([(CONTENT_TYPE, content_type), (CACHE_CONTROL, "no-cache")], body)
}

/// Handle one webhook update. A failure answers 500, so that Telegram sends the update again later.
async fn receive_update<S: FeedStore, M: BotMessenger>(
    State(webhook): State<Arc<Webhook<S, M>>>,
    headers: HeaderMap,
    update: Result<Json<Update>, JsonRejection>,
) -> Result<&'static str, StatusCode> {
    let secret_token = headers.get(WEBHOOK_SECRET_HEADER).map(|value| value.as_bytes()).unwrap_or_default();
    if !bool::from(secret_token.ct_eq(webhook.secret_token.as_bytes())) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let Json(update) = update.map_err(|rejection| rejection.status())?;
    webhook.commands.handle_update(&update).await.map_err(|update_error| {
        error!("Could not handle Telegram update {}: {update_error:#}", update.update_id);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok("ok")
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    use super::*;
    use crate::entrypoints::commands::tests::{ADMIN_ID, handler};
    use crate::testing::FakeStore;

    fn app(store: &Arc<FakeStore>) -> Router {
        let (commands, _) = handler(store);
        let webhook = Webhook { path: "/telegram/webhook".to_owned(), secret_token: "secret".to_owned(), commands };
        create_app(Router::new()).merge(webhook.into_router())
    }

    async fn status(app: &Router, request: Request<Body>) -> StatusCode {
        app.clone().oneshot(request).await.unwrap().status()
    }

    fn webhook_request(secret_token: &str) -> Request<Body> {
        let update = format!(
            r#"{{"update_id": 1, "message": {{"chat": {{"id": 5}}, "from": {{"id": {ADMIN_ID}}}, "text": "/start"}}}}"#
        );
        Request::post("/telegram/webhook")
            .header(WEBHOOK_SECRET_HEADER, secret_token)
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(update))
            .unwrap()
    }

    #[tokio::test]
    async fn the_site_serves_health_checks_and_the_mini_app() {
        let app = app(&Arc::new(FakeStore::default()));
        let get = |uri: &str| Request::get(uri).body(Body::empty()).unwrap();

        let page = app.clone().oneshot(get("/app/")).await.unwrap();
        let redirect = app.clone().oneshot(get("/")).await.unwrap();

        assert_eq!(status(&app, get("/healthz")).await, StatusCode::OK);
        assert_eq!(page.headers()[CONTENT_TYPE], "text/html; charset=utf-8");
        assert_eq!(redirect.headers()["location"], "/app/");
        assert_eq!(status(&app, get("/app/app.js")).await, StatusCode::OK);
        assert_eq!(status(&app, get("/app/missing.js")).await, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn webhook_updates_need_the_secret_token() {
        let store = Arc::new(FakeStore::default());
        let app = app(&store);

        assert_eq!(status(&app, webhook_request("wrong")).await, StatusCode::UNAUTHORIZED);
        assert_eq!(store.destination(), None);
        assert_eq!(status(&app, webhook_request("secret")).await, StatusCode::OK);
        assert_eq!(store.destination().as_deref(), Some("5"));
    }
}
