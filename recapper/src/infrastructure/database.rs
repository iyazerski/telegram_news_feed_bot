use anyhow::Result;
use chrono::{DateTime, Utc};
use common::database::connect_lazy;
use common::settings::{
    DESTINATION_CHAT_ID_KEY, RECAP_KEYS, RecapSchedule, RecapSettings, load_settings, parse_recap_schedule,
    store_recap_next_run_at,
};
use sqlx::Row;
use sqlx::postgres::PgPool;

use crate::domain::recaps::RecapStory;

// One loop does one thing at a time; the second connection only covers a reconnect.
const MAX_CONNECTIONS: u32 = 2;

/// The stored recap configuration, schedule, and destination.
pub struct StoredRecap {
    pub settings: RecapSettings,
    pub next_run_at: Option<DateTime<Utc>>,
    pub destination: Option<String>,
}

/// Recap settings and sent-story history in the shared Postgres database.
pub struct PostgresRecapStore {
    pool: PgPool,
}

impl PostgresRecapStore {
    /// Create a store over a lazily connected pool, so the service starts while the database is down.
    pub fn connect_lazy(database_url: &str) -> Result<Self> {
        Ok(Self { pool: connect_lazy(database_url, MAX_CONNECTIONS)? })
    }

    /// Read the recap settings, next run time, and destination chat.
    pub async fn load_recap(&self, default_interval_seconds: i64) -> Result<StoredRecap> {
        let mut keys = RECAP_KEYS.to_vec();
        keys.push(DESTINATION_CHAT_ID_KEY);
        let mut values = load_settings(&self.pool, &keys).await?;
        let RecapSchedule { settings, next_run_at } = parse_recap_schedule(&values, default_interval_seconds)?;
        Ok(StoredRecap { settings, next_run_at, destination: values.remove(DESTINATION_CHAT_ID_KEY) })
    }

    /// Return the newest sent stories as (title, url) pairs.
    pub async fn load_sent_stories(&self, limit: i64) -> Result<Vec<(String, String)>> {
        let rows = sqlx::query("SELECT title, url FROM sent_recap_story ORDER BY sent_at DESC LIMIT $1")
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(|row| (row.get("title"), row.get("url"))).collect())
    }

    /// Remember sent stories, prune history older than `prune_before`, and schedule the next recap, atomically.
    pub async fn commit_recap(
        &self,
        stories: &[RecapStory],
        sent_at: DateTime<Utc>,
        prune_before: DateTime<Utc>,
        next_run_at: DateTime<Utc>,
    ) -> Result<()> {
        let mut transaction = self.pool.begin().await?;
        for story in stories {
            sqlx::query("INSERT INTO sent_recap_story (url, title, sent_at) VALUES ($1, $2, $3)")
                .bind(&story.url)
                .bind(&story.title)
                .bind(sent_at)
                .execute(&mut *transaction)
                .await?;
        }
        sqlx::query("DELETE FROM sent_recap_story WHERE sent_at < $1")
            .bind(prune_before)
            .execute(&mut *transaction)
            .await?;
        store_recap_next_run_at(&mut *transaction, next_run_at).await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Store when the next recap attempt runs.
    pub async fn schedule(&self, run_at: DateTime<Utc>) -> Result<()> {
        store_recap_next_run_at(&self.pool, run_at).await
    }
}
