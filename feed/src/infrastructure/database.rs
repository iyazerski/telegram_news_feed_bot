use std::time::Duration;

use anyhow::{Context, Result};
use common::database::connect_lazy;
use common::settings::{DESTINATION_CHAT_ID_KEY, POLL_INTERVAL_SECONDS_KEY, load_settings};
use sqlx::Row;
use sqlx::postgres::PgPool;

use crate::domain::posts::SourceChannel;
use crate::use_cases::forward_posts::{FeedSettings, FeedStore};

// One loop does one thing at a time; the second connection only covers a reconnect.
const MAX_CONNECTIONS: u32 = 2;

/// Channel subscriptions, their cursors, and feed settings in the shared Postgres database.
pub struct PostgresFeedStore {
    pool: PgPool,
}

impl PostgresFeedStore {
    /// Create a store over a lazily connected pool, so the service starts while the database is down.
    pub fn connect_lazy(database_url: &str) -> Result<Self> {
        Ok(Self { pool: connect_lazy(database_url, MAX_CONNECTIONS)? })
    }
}

impl FeedStore for PostgresFeedStore {
    async fn load_settings(&self, default_poll_interval_seconds: u64) -> Result<FeedSettings> {
        let mut values = load_settings(&self.pool, &[DESTINATION_CHAT_ID_KEY, POLL_INTERVAL_SECONDS_KEY]).await?;
        let poll_interval_seconds = match values.get(POLL_INTERVAL_SECONDS_KEY) {
            Some(seconds) => seconds.parse().context("invalid stored poll interval")?,
            None => default_poll_interval_seconds,
        };
        Ok(FeedSettings {
            destination: values.remove(DESTINATION_CHAT_ID_KEY),
            poll_interval: Duration::from_secs(poll_interval_seconds),
        })
    }

    async fn list_active_channels(&self, after: &str, limit: i64) -> Result<Vec<SourceChannel>> {
        let rows = sqlx::query(
            "SELECT username, last_committed_message_id FROM source_channel \
             WHERE active AND username > $1 ORDER BY username LIMIT $2",
        )
        .bind(after)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| SourceChannel {
                username: row.get("username"),
                last_committed_message_id: row.get("last_committed_message_id"),
            })
            .collect())
    }

    async fn commit_post(&self, source_channel: &str, message_id: i32) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE source_channel SET last_committed_message_id = $2, updated_at = now() \
             WHERE username = $1 AND active AND last_committed_message_id < $2",
        )
        .bind(source_channel)
        .bind(message_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}
