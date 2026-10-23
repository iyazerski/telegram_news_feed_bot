use std::time::Duration;

use crate::use_cases::forward_posts::PreviewSource;

const PRIMARY_PREVIEW_URL: &str = "https://t.me";
const FALLBACK_PREVIEW_URL: &str = "https://telegram.me";

/// A channel web preview that could not be fetched from either Telegram host.
#[derive(Debug, thiserror::Error)]
#[error("Telegram web preview is unavailable for @{username}: {reason}")]
pub struct PreviewUnavailableError {
    pub username: String,
    pub reason: String,
}

/// Client for public Telegram channel web preview pages.
pub struct TelegramWebPreviewClient {
    http: reqwest::Client,
    primary_url: String,
    fallback_url: String,
}

impl TelegramWebPreviewClient {
    /// Create a client whose requests give up after `timeout`.
    pub fn new(timeout: Duration) -> anyhow::Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder().timeout(timeout).build()?,
            primary_url: PRIMARY_PREVIEW_URL.to_owned(),
            fallback_url: FALLBACK_PREVIEW_URL.to_owned(),
        })
    }

    async fn fetch_from(&self, base_url: &str, username: &str) -> reqwest::Result<String> {
        self.http.get(format!("{base_url}/s/{username}")).send().await?.error_for_status()?.text().await
    }
}

impl PreviewSource for TelegramWebPreviewClient {
    /// Fetch a channel's public web preview page, trying the fallback host once when the primary is unreachable.
    async fn fetch_channel_preview(&self, username: &str) -> Result<String, PreviewUnavailableError> {
        let page = match self.fetch_from(&self.primary_url, username).await {
            Err(error) if error.is_connect() => self.fetch_from(&self.fallback_url, username).await,
            page => page,
        };
        page.map_err(|error| PreviewUnavailableError {
            username: username.to_owned(),
            // The error chain names the cause, such as a DNS failure, under the request error.
            reason: format!("{:#}", anyhow::Error::from(error)),
        })
    }
}

#[cfg(test)]
mod tests {
    use common::testing::{CannedResponse, serve};

    use super::*;

    /// Return a local URL that refuses connections.
    async fn unreachable_url() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        format!("http://{}", listener.local_addr().unwrap())
    }

    fn client(primary_url: String, fallback_url: String) -> TelegramWebPreviewClient {
        TelegramWebPreviewClient { http: reqwest::Client::new(), primary_url, fallback_url }
    }

    #[tokio::test]
    async fn unreachable_primary_host_falls_back_once() {
        let (fallback_url, server) = serve(vec![CannedResponse::new(200, "text/html", "<html>page</html>")]).await;

        let page = client(unreachable_url().await, fallback_url).fetch_channel_preview("example").await.unwrap();

        assert_eq!(page, "<html>page</html>");
        assert!(server.await.unwrap()[0].starts_with("GET /s/example "));
    }

    #[tokio::test]
    async fn answered_errors_and_unreachable_hosts_make_the_preview_unavailable() {
        // An error answer does not try the other host, and two unreachable hosts give up.
        let (primary_url, server) = serve(vec![CannedResponse::new(500, "text/plain", "error")]).await;
        let answered = client(primary_url, unreachable_url().await).fetch_channel_preview("example").await;
        let unreachable =
            client(unreachable_url().await, unreachable_url().await).fetch_channel_preview("example").await;

        assert_eq!(answered.unwrap_err().username, "example");
        assert_eq!(unreachable.unwrap_err().username, "example");
        server.await.unwrap();
    }
}
