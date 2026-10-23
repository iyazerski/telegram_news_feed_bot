use std::time::Duration;

use anyhow::Result;
use common::runtime::run_until_shutdown;
use common::telegram::bot_api::TelegramBotApi;
use tracing::{error, info};

use crate::config::Configs;
use crate::infrastructure::database::PostgresFeedStore;
use crate::infrastructure::telegram::media_downloader::TelegramMediaDownloader;
use crate::infrastructure::telegram::web_preview_client::TelegramWebPreviewClient;
use crate::use_cases::forward_posts::PostForwardingService;

/// Wire the forwarding service and run its cycles until the process is told to stop.
pub async fn run_forever(configs: Configs) -> Result<()> {
    let forwarding = PostForwardingService::new(
        PostgresFeedStore::connect_lazy(&configs.database_url)?,
        TelegramWebPreviewClient::new(configs.preview_http_timeout())?,
        TelegramMediaDownloader::new(configs.telegram_http_timeout(), configs.post_media_max_bytes)?,
        TelegramBotApi::new(&configs.bot_token, configs.telegram_http_timeout())?,
        configs.default_poll_interval_seconds,
    );
    let default_interval = Duration::from_secs(configs.default_poll_interval_seconds);

    info!("Feed started");
    // A post in delivery stops with the process; its cursor is not committed, so the next start resends it.
    run_until_shutdown(|| async {
        forwarding.run_cycle().await.unwrap_or_else(|cycle_error| {
            // Keep the service alive through transient database failures.
            error!("Feed cycle failed; retrying after the default interval: {cycle_error:#}");
            default_interval
        })
    })
    .await;
    info!("Feed stopped");
    Ok(())
}
