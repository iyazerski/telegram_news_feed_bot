use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use common::database::{DESTINATION_CHAT_ID_KEY, connect_lazy};
use sqlx::postgres::PgPool;
use sqlx::{Executor, Postgres, Row};

use crate::domain::recaps::{RecapSettings, RecapStory};

// Setting keys shared with the Python services' SettingsService.
const RECAP_TOPICS_KEY: &str = "recap_topics";
const RECAP_LANGUAGE_KEY: &str = "recap_language";
const RECAP_INTERVAL_SECONDS_KEY: &str = "recap_interval_seconds";
const RECAP_ENABLED_KEY: &str = "recap_enabled";
const RECAP_NEXT_RUN_AT_KEY: &str = "recap_next_run_at";
const DEFAULT_RECAP_LANGUAGE: &str = "English";
// One loop does one thing at a time; the second connection only covers a reconnect.
const MAX_CONNECTIONS: u32 = 2;

/// The stored recap configuration, schedule, and destination.
pub struct StoredRecap {
    pub settings: RecapSettings,
    pub next_run_at: Option<DateTime<Utc>>,
    pub destination: Option<String>,
}

/// Recap settings and sent-story history in the shared Postgres database, whose schema Alembic owns.
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
        let keys = [
            DESTINATION_CHAT_ID_KEY,
            RECAP_TOPICS_KEY,
            RECAP_LANGUAGE_KEY,
            RECAP_INTERVAL_SECONDS_KEY,
            RECAP_ENABLED_KEY,
            RECAP_NEXT_RUN_AT_KEY,
        ];
        let rows = sqlx::query("SELECT key, value FROM app_setting WHERE key = ANY($1)")
            .bind(&keys[..])
            .fetch_all(&self.pool)
            .await?;
        let mut values: HashMap<String, String> =
            rows.into_iter().map(|row| (row.get("key"), row.get("value"))).collect();
        let topics = match values.remove(RECAP_TOPICS_KEY) {
            Some(topics) => serde_json::from_str(&topics).context("invalid stored recap topics")?,
            None => Vec::new(),
        };
        let interval_seconds = match values.remove(RECAP_INTERVAL_SECONDS_KEY) {
            Some(seconds) => seconds.parse().context("invalid stored recap interval")?,
            None => default_interval_seconds,
        };
        let next_run_at = match values.remove(RECAP_NEXT_RUN_AT_KEY).filter(|value| !value.is_empty()) {
            Some(value) => Some(DateTime::parse_from_rfc3339(&value).context("invalid stored recap time")?.to_utc()),
            None => None,
        };
        Ok(StoredRecap {
            settings: RecapSettings {
                topics,
                language: values
                    .remove(RECAP_LANGUAGE_KEY)
                    .filter(|language| !language.is_empty())
                    .unwrap_or_else(|| DEFAULT_RECAP_LANGUAGE.to_owned()),
                interval_seconds,
                enabled: values.get(RECAP_ENABLED_KEY).is_some_and(|enabled| enabled == "true"),
            },
            next_run_at,
            destination: values.remove(DESTINATION_CHAT_ID_KEY),
        })
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
        set_next_run_at(&mut *transaction, next_run_at).await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Store when the next recap attempt runs.
    pub async fn schedule(&self, run_at: DateTime<Utc>) -> Result<()> {
        set_next_run_at(&self.pool, run_at).await
    }
}

/// Upsert the next run time in the format Python's `datetime.fromisoformat` reads.
async fn set_next_run_at<'e>(executor: impl Executor<'e, Database = Postgres>, run_at: DateTime<Utc>) -> Result<()> {
    sqlx::query(
        "INSERT INTO app_setting (key, value, updated_at) VALUES ($1, $2, now()) \
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(RECAP_NEXT_RUN_AT_KEY)
    .bind(run_at.to_rfc3339_opts(SecondsFormat::Micros, false))
    .execute(executor)
    .await?;
    Ok(())
}
