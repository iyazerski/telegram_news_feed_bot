//! Migration job: brings the shared Postgres database up to the newest schema in `migrations/`, which every service
//! expects before it starts.

use anyhow::Result;
use common::config::{default_database_url, default_log_level};
use serde::Deserialize;
use sqlx::migrate::Migrator;
use tracing::info;

static MIGRATOR: Migrator = sqlx::migrate!("../migrations");

/// Migration settings, read from `TELEGRAM_NEWS_*` variables.
#[derive(Debug, Deserialize)]
struct Configs {
    #[serde(default = "default_database_url")]
    database_url: String,
    #[serde(default = "default_log_level")]
    log_level: String,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let configs: Configs = common::config::load()?;
    common::runtime::configure_logging(&configs.log_level);
    let pool = common::database::connect_lazy(&configs.database_url, 1)?;
    // Each migration runs in its own transaction, so a failed one leaves the database as it was before it.
    MIGRATOR.run(&pool).await?;
    info!("The database schema is up to date");
    Ok(())
}
