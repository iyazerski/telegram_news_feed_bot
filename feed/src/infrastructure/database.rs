use std::time::Duration;

use anyhow::{Context, Result};
use common::database::{DESTINATION_CHAT_ID_KEY, connect_lazy};
use sqlx::Row;
use sqlx::postgres::PgPool;

use crate::domain::posts::SourceChannel;

// Setting key shared with the Python services' SettingsService.
const POLL_INTERVAL_SECONDS_KEY: &str = "poll_interval_seconds";
// One loop does one thing at a time; the second connection only covers a reconnect.
const MAX_CONNECTIONS: u32 = 2;

/// The stored destination chat and poll interval.
pub struct FeedSettings {
    pub destination: Option<String>,
    pub poll_interval: Duration,
}

/// Channel subscriptions, their cursors, and feed settings in the shared Postgres database, whose schema Alembic owns.
pub struct PostgresFeedStore {
    pool: PgPool,
}

impl PostgresFeedStore {
    /// Create a store over a lazily connected pool, so the service starts while the database is down.
    pub fn connect_lazy(database_url: &str) -> Result<Self> {
        Ok(Self { pool: connect_lazy(database_url, MAX_CONNECTIONS)? })
    }

    /// Read the destination chat and the poll interval, which falls back to the default before the first save.
    pub async fn load_settings(&self, default_poll_interval_seconds: u64) -> Result<FeedSettings> {
        let rows = sqlx::query("SELECT key, value FROM app_setting WHERE key = ANY($1)")
            .bind(&[DESTINATION_CHAT_ID_KEY, POLL_INTERVAL_SECONDS_KEY][..])
            .fetch_all(&self.pool)
            .await?;
        let mut settings =
            FeedSettings { destination: None, poll_interval: Duration::from_secs(default_poll_interval_seconds) };
        for row in rows {
            let value: String = row.get("value");
            if row.get::<&str, _>("key") == POLL_INTERVAL_SECONDS_KEY {
                settings.poll_interval = Duration::from_secs(value.parse().context("invalid stored poll interval")?);
            } else {
                settings.destination = Some(value);
            }
        }
        Ok(settings)
    }

    /// Return up to `limit` active channels after the `after` username, in username order.
    pub async fn list_active_channels(&self, after: &str, limit: i64) -> Result<Vec<SourceChannel>> {
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

    /// Move an active channel's cursor forward to `message_id`, and return false when the channel is not active.
    pub async fn commit_post(&self, source_channel: &str, message_id: i32) -> Result<bool> {
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
