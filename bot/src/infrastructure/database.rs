use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use common::database::connect_lazy;
use common::settings::{
    DESTINATION_CHAT_ID_KEY, POLL_INTERVAL_SECONDS_KEY, RECAP_KEYS, RecapSchedule, RecapSettings, load_settings,
    parse_recap_schedule, store_recap_next_run_at, store_recap_settings, store_setting,
};
use sqlx::postgres::PgPool;

use crate::use_cases::manage_feed::FeedStore;
use crate::use_cases::manage_recaps::RecapStore;

// Mini App requests can overlap with an update, so a few connections keep them from queueing.
const MAX_CONNECTIONS: u32 = 4;

/// Feed settings, channel subscriptions, and recap settings in the shared Postgres database.
pub struct PostgresBotStore {
    pool: PgPool,
}

impl PostgresBotStore {
    /// Create a store over a lazily connected pool, so the service starts while the database is down.
    pub fn connect_lazy(database_url: &str) -> Result<Self> {
        Ok(Self { pool: connect_lazy(database_url, MAX_CONNECTIONS)? })
    }
}

impl FeedStore for PostgresBotStore {
    async fn store_destination_chat_id(&self, chat_id: &str) -> Result<()> {
        store_setting(&self.pool, DESTINATION_CHAT_ID_KEY, chat_id).await
    }

    async fn delete_destination_chat_id(&self, chat_id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM app_setting WHERE key = $1 AND value = $2")
            .bind(DESTINATION_CHAT_ID_KEY)
            .bind(chat_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn load_poll_interval_seconds(&self) -> Result<Option<u64>> {
        let values = load_settings(&self.pool, &[POLL_INTERVAL_SECONDS_KEY]).await?;
        values
            .get(POLL_INTERVAL_SECONDS_KEY)
            .map(|seconds| seconds.parse().context("invalid stored poll interval"))
            .transpose()
    }

    async fn store_poll_interval_seconds(&self, seconds: u64) -> Result<()> {
        store_setting(&self.pool, POLL_INTERVAL_SECONDS_KEY, &seconds.to_string()).await
    }

    async fn list_active_channels(&self, after: &str, limit: usize) -> Result<Vec<String>> {
        // The (active, username) index serves this filter and order.
        let usernames = sqlx::query_scalar(
            "SELECT username FROM source_channel WHERE active AND username > $1 ORDER BY username LIMIT $2",
        )
        .bind(after)
        .bind(i64::try_from(limit)?)
        .fetch_all(&self.pool)
        .await?;
        Ok(usernames)
    }

    async fn activate_channel(&self, username: &str) -> Result<()> {
        // A returning channel keeps its cursor, so the feed does not resend posts it already delivered.
        sqlx::query(
            "INSERT INTO source_channel (username) VALUES ($1) \
             ON CONFLICT (username) DO UPDATE SET active = true, updated_at = now() WHERE NOT source_channel.active",
        )
        .bind(username)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn deactivate_channel(&self, username: &str) -> Result<bool> {
        let result = sqlx::query("UPDATE source_channel SET active = false, updated_at = now() WHERE username = $1")
            .bind(username)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

impl RecapStore for PostgresBotStore {
    async fn load_recap(&self, default_interval_seconds: i64) -> Result<RecapSchedule> {
        let values = load_settings(&self.pool, &RECAP_KEYS).await?;
        parse_recap_schedule(&values, default_interval_seconds)
    }

    async fn store_recap(&self, settings: &RecapSettings, next_run_at: Option<DateTime<Utc>>) -> Result<()> {
        let mut transaction = self.pool.begin().await?;
        store_recap_settings(&mut transaction, settings).await?;
        if let Some(run_at) = next_run_at {
            store_recap_next_run_at(&mut *transaction, run_at).await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    async fn store_recap_next_run_at(&self, run_at: DateTime<Utc>) -> Result<()> {
        store_recap_next_run_at(&self.pool, run_at).await
    }
}
