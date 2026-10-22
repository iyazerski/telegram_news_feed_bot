use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use tracing::{error, info};

use crate::config::Configs;
use crate::infrastructure::ai::researcher::ChatGptResearcher;
use crate::infrastructure::database::PostgresRecapStore;
use crate::infrastructure::telegram::bot_api::TelegramBotApi;
use crate::use_cases::generate_recaps::{NewsRecapService, RecapPolicy};

/// Wire the recap service and check its schedule until the process is told to stop.
pub async fn run_forever(configs: Configs) -> Result<()> {
    configs.require_service_settings()?;
    let recaps = NewsRecapService::new(
        PostgresRecapStore::connect_lazy(&configs.postgres_url())?,
        ChatGptResearcher::new(
            &configs.recap_auth_dir,
            &configs.recap_tavily_api_key,
            Duration::from_secs(configs.recap_timeout_seconds),
        ),
        TelegramBotApi::new(&configs.bot_token, configs.telegram_http_timeout())?,
        RecapPolicy::from_configs(&configs),
    );
    let check_interval = Duration::from_secs(configs.recap_check_interval_seconds);

    info!("Recapper started");
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            // A recap in progress stops with the process; its schedule is unchanged, so the next start retries it.
            () = &mut shutdown => break,
            result = recaps.run_if_due(Utc::now()) => {
                if let Err(check_error) = result {
                    // Keep the service alive through transient database failures.
                    error!("Recap check failed; retrying on the next check: {check_error:#}");
                }
            }
        }
        tokio::select! {
            () = &mut shutdown => break,
            () = tokio::time::sleep(check_interval) => {}
        }
    }
    info!("Recapper stopped");
    Ok(())
}

/// Resolve on SIGTERM, which Kubernetes sends before it kills the pod, or on Ctrl-C locally.
async fn shutdown_signal() {
    let mut terminate = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        Ok(terminate) => terminate,
        Err(signal_error) => {
            error!("Could not listen for SIGTERM: {signal_error}");
            let _ = tokio::signal::ctrl_c().await;
            return;
        }
    };
    tokio::select! {
        _ = terminate.recv() => {}
        _ = tokio::signal::ctrl_c() => {}
    }
}
