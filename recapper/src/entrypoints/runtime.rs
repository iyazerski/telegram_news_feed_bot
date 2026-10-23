use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use common::runtime::run_until_shutdown;
use common::telegram::bot_api::TelegramBotApi;
use tracing::{error, info};

use crate::config::Configs;
use crate::infrastructure::ai::researcher::ChatGptResearcher;
use crate::infrastructure::database::PostgresRecapStore;
use crate::use_cases::generate_recaps::{NewsRecapService, RecapPolicy};

/// Wire the recap service and check its schedule until the process is told to stop.
pub async fn run_forever(configs: Configs) -> Result<()> {
    let recaps = NewsRecapService::new(
        PostgresRecapStore::connect_lazy(&configs.database_url)?,
        ChatGptResearcher::new(
            &configs.recap_auth_dir,
            &configs.recap_tavily_api_key,
            Duration::from_secs(configs.recap_timeout_seconds),
        )?,
        TelegramBotApi::new(&configs.bot_token, configs.telegram_http_timeout())?,
        RecapPolicy::from_configs(&configs),
    );
    let check_interval = Duration::from_secs(configs.recap_check_interval_seconds);

    info!("Recapper started");
    // A recap in progress stops with the process; its schedule is unchanged, so the next start retries it.
    run_until_shutdown(|| async {
        if let Err(check_error) = recaps.run_if_due(Utc::now()).await {
            // Keep the service alive through transient database failures.
            error!("Recap check failed; retrying on the next check: {check_error:#}");
        }
        check_interval
    })
    .await;
    info!("Recapper stopped");
    Ok(())
}
