use std::future::IntoFuture;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use common::runtime::shutdown_signal;
use common::telegram::bot_api::TelegramBotApi;
use tokio::net::TcpListener;
use tracing::{info, warn};

use crate::config::{Configs, UpdateMode};
use crate::entrypoints::commands::CommandHandler;
use crate::entrypoints::http::mini_app::{MiniApp, MiniAppAuth};
use crate::entrypoints::http::{Webhook, create_app};
use crate::entrypoints::polling::UpdatePoller;
use crate::infrastructure::database::PostgresBotStore;
use crate::use_cases::manage_feed::FeedService;
use crate::use_cases::manage_recaps::RecapService;

/// Wire the bot, receive updates the configured way, and serve the Mini App until the process is told to stop.
pub async fn run_forever(configs: Configs) -> Result<()> {
    configs.require_service_settings()?;
    let store = Arc::new(PostgresBotStore::connect_lazy(&configs.database_url)?);
    let telegram = TelegramBotApi::new(&configs.bot_token, configs.telegram_http_timeout())?;
    let feed = || FeedService::new(Arc::clone(&store), configs.default_poll_interval_seconds);
    let commands = CommandHandler::new(feed(), telegram.clone(), configs.admin_user_id);
    let mini_app = MiniApp::new(
        feed(),
        RecapService::new(Arc::clone(&store), configs.default_recap_interval_seconds),
        MiniAppAuth::new(&configs.bot_token, configs.admin_user_id, configs.mini_app_auth_max_age_seconds),
    )
    .into_router();
    let app = create_app(mini_app);
    configure_menu_button(&configs, &telegram).await;

    let listener = TcpListener::bind((configs.bot_webhook_listen_host.as_str(), configs.bot_webhook_port)).await?;
    info!("Bot HTTP service started on {}", listener.local_addr()?);
    match configs.bot_update_mode {
        UpdateMode::Webhook => {
            let webhook_url = configs.webhook_url()?;
            telegram.set_webhook(&webhook_url, &configs.bot_webhook_secret_token).await?;
            info!("Receiving Telegram updates at {webhook_url}");
            let webhook = Webhook {
                path: configs.bot_webhook_path.clone(),
                secret_token: configs.bot_webhook_secret_token.clone(),
                commands,
            };
            axum::serve(listener, app.merge(webhook.into_router())).with_graceful_shutdown(shutdown_signal()).await?;
        }
        UpdateMode::Polling => {
            if configs.bot_delete_webhook_on_polling_start {
                telegram.delete_webhook().await?;
            }
            let poller = UpdatePoller::new(
                commands,
                telegram,
                Duration::from_secs(configs.telegram_get_updates_timeout_seconds),
                Duration::from_secs(configs.telegram_get_updates_retry_seconds),
            );
            let server = axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).into_future();
            // The server stops on the shutdown signal, which also ends polling.
            tokio::select! {
                result = server => result?,
                () = poller.run_forever() => {}
            }
        }
    }
    info!("Bot stopped");
    Ok(())
}

/// Point the bot's menu button at the Mini App when it has a public URL.
async fn configure_menu_button(configs: &Configs, telegram: &TelegramBotApi) {
    let Some(mini_app_url) = configs.mini_app_url().filter(|_| configs.bot_configure_menu_button) else {
        return;
    };
    // Telegram keeps the button from earlier starts, so a failure here need not stop the bot.
    match telegram.set_chat_menu_button("Manage Feed", &mini_app_url).await {
        Ok(()) => info!("Configured the Telegram menu button for {mini_app_url}"),
        Err(menu_error) => warn!("Could not configure the Telegram menu button: {menu_error}"),
    }
}
