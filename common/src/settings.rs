//! The `app_setting` key-value table: the bot writes the settings, and the feed reads them.

use std::collections::HashMap;

use anyhow::Result;
use sqlx::{Executor, Postgres, Row};

/// The bot chat that receives posts. The bot stores it on /start and deletes it when the chat blocks or removes the
/// bot, which pauses the feed until the next /start.
pub const DESTINATION_CHAT_ID_KEY: &str = "destination_chat_id";
pub const POLL_INTERVAL_SECONDS_KEY: &str = "poll_interval_seconds";
/// Return the stored values of `keys`, without the keys that were never saved.
pub async fn load_settings<'e>(
    executor: impl Executor<'e, Database = Postgres>,
    keys: &[&str],
) -> Result<HashMap<String, String>> {
    let rows =
        sqlx::query("SELECT key, value FROM app_setting WHERE key = ANY($1)").bind(keys).fetch_all(executor).await?;
    Ok(rows.into_iter().map(|row| (row.get("key"), row.get("value"))).collect())
}

/// Create or replace one setting value.
pub async fn store_setting<'e>(executor: impl Executor<'e, Database = Postgres>, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO app_setting (key, value, updated_at) VALUES ($1, $2, now()) \
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(key)
    .bind(value)
    .execute(executor)
    .await?;
    Ok(())
}
