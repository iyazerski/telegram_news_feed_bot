use anyhow::Result;
use chrono::{DateTime, Utc};
use common::database::connect_lazy;
use common::settings::{
    DESTINATION_CHAT_ID_KEY, RECAP_KEYS, load_settings, parse_recap_schedule, store_recap_next_run_at,
};
use sqlx::Row;
use sqlx::postgres::PgPool;

use crate::domain::recaps::{RecapStory, SentStory};
use crate::use_cases::generate_recaps::{RecapStore, StoredRecap};

// One loop does one thing at a time; the second connection only covers a reconnect.
const MAX_CONNECTIONS: u32 = 2;

/// Recap settings and sent-story history in the shared Postgres database.
pub struct PostgresRecapStore {
    pool: PgPool,
}

impl PostgresRecapStore {
    /// Create a store over a lazily connected pool, so the service starts while the database is down.
    pub fn connect_lazy(database_url: &str) -> Result<Self> {
        Ok(Self { pool: connect_lazy(database_url, MAX_CONNECTIONS)? })
    }
}

impl RecapStore for PostgresRecapStore {
    async fn load_recap(&self, default_interval_seconds: i64) -> Result<StoredRecap> {
        let mut keys = RECAP_KEYS.to_vec();
        keys.push(DESTINATION_CHAT_ID_KEY);
        let mut values = load_settings(&self.pool, &keys).await?;
        let schedule = parse_recap_schedule(&values, default_interval_seconds)?;
        Ok(StoredRecap { schedule, destination: values.remove(DESTINATION_CHAT_ID_KEY) })
    }

    async fn load_sent_stories(&self, limit: usize) -> Result<Vec<SentStory>> {
        let rows = sqlx::query("SELECT title, url FROM sent_recap_story ORDER BY sent_at DESC LIMIT $1")
            .bind(i64::try_from(limit)?)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(|row| SentStory { title: row.get("title"), url: row.get("url") }).collect())
    }

    async fn commit_recap(
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

    async fn schedule(&self, run_at: DateTime<Utc>) -> Result<()> {
        store_recap_next_run_at(&self.pool, run_at).await
    }
}
