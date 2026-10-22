//! The `app_setting` key-value table: the bot writes the settings, and the feed and recapper read them.

use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use sqlx::postgres::PgConnection;
use sqlx::{Executor, Postgres, Row};

/// The bot chat that receives posts and recaps, which the bot stores on /start.
pub const DESTINATION_CHAT_ID_KEY: &str = "destination_chat_id";
pub const POLL_INTERVAL_SECONDS_KEY: &str = "poll_interval_seconds";
const RECAP_TOPICS_KEY: &str = "recap_topics";
const RECAP_LANGUAGE_KEY: &str = "recap_language";
const RECAP_INTERVAL_SECONDS_KEY: &str = "recap_interval_seconds";
const RECAP_ENABLED_KEY: &str = "recap_enabled";
const RECAP_NEXT_RUN_AT_KEY: &str = "recap_next_run_at";
/// The keys that `parse_recap_schedule` reads.
pub const RECAP_KEYS: [&str; 5] =
    [RECAP_TOPICS_KEY, RECAP_LANGUAGE_KEY, RECAP_INTERVAL_SECONDS_KEY, RECAP_ENABLED_KEY, RECAP_NEXT_RUN_AT_KEY];
const DEFAULT_RECAP_LANGUAGE: &str = "English";

/// The recap settings the Mini App stores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecapSettings {
    pub topics: Vec<String>,
    pub language: String,
    pub interval_seconds: i64,
    pub enabled: bool,
}

/// The recap settings and when the next recap is due, if one is scheduled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecapSchedule {
    pub settings: RecapSettings,
    pub next_run_at: Option<DateTime<Utc>>,
}

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

/// Read the recap settings and schedule from loaded values, with recaps off before the first save.
pub fn parse_recap_schedule(values: &HashMap<String, String>, default_interval_seconds: i64) -> Result<RecapSchedule> {
    let topics = match values.get(RECAP_TOPICS_KEY) {
        Some(topics) => serde_json::from_str(topics).context("invalid stored recap topics")?,
        None => Vec::new(),
    };
    let interval_seconds = match values.get(RECAP_INTERVAL_SECONDS_KEY) {
        Some(seconds) => seconds.parse().context("invalid stored recap interval")?,
        None => default_interval_seconds,
    };
    let next_run_at = match values.get(RECAP_NEXT_RUN_AT_KEY).filter(|value| !value.is_empty()) {
        Some(value) => Some(DateTime::parse_from_rfc3339(value).context("invalid stored recap time")?.to_utc()),
        None => None,
    };
    let language = values
        .get(RECAP_LANGUAGE_KEY)
        .filter(|language| !language.is_empty())
        .map_or(DEFAULT_RECAP_LANGUAGE, String::as_str)
        .to_owned();
    Ok(RecapSchedule {
        settings: RecapSettings {
            topics,
            language,
            interval_seconds,
            enabled: values.get(RECAP_ENABLED_KEY).is_some_and(|enabled| enabled == "true"),
        },
        next_run_at,
    })
}

/// Store the recap settings; run it in a transaction so readers never see half of a save.
pub async fn store_recap_settings(connection: &mut PgConnection, settings: &RecapSettings) -> Result<()> {
    let topics = serde_json::to_string(&settings.topics)?;
    store_setting(&mut *connection, RECAP_TOPICS_KEY, &topics).await?;
    store_setting(&mut *connection, RECAP_LANGUAGE_KEY, &settings.language).await?;
    store_setting(&mut *connection, RECAP_INTERVAL_SECONDS_KEY, &settings.interval_seconds.to_string()).await?;
    store_setting(&mut *connection, RECAP_ENABLED_KEY, if settings.enabled { "true" } else { "false" }).await
}

/// Store when the next recap is due.
pub async fn store_recap_next_run_at<'e>(
    executor: impl Executor<'e, Database = Postgres>,
    run_at: DateTime<Utc>,
) -> Result<()> {
    store_setting(executor, RECAP_NEXT_RUN_AT_KEY, &run_at.to_rfc3339_opts(SecondsFormat::Micros, false)).await
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn recap_schedule_defaults_to_off_and_reads_saved_values() {
        let defaults = parse_recap_schedule(&HashMap::new(), 21_600).unwrap();
        let saved: HashMap<String, String> = [
            (RECAP_TOPICS_KEY, r#"["AI", "Космос"]"#),
            (RECAP_LANGUAGE_KEY, "Russian"),
            (RECAP_INTERVAL_SECONDS_KEY, "43200"),
            (RECAP_ENABLED_KEY, "true"),
            (RECAP_NEXT_RUN_AT_KEY, "2026-10-22T19:44:10.123456+00:00"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect();

        assert_eq!(
            defaults,
            RecapSchedule {
                settings: RecapSettings {
                    topics: Vec::new(),
                    language: "English".to_owned(),
                    interval_seconds: 21_600,
                    enabled: false,
                },
                next_run_at: None,
            }
        );
        assert_eq!(
            parse_recap_schedule(&saved, 21_600).unwrap(),
            RecapSchedule {
                settings: RecapSettings {
                    topics: vec!["AI".to_owned(), "Космос".to_owned()],
                    language: "Russian".to_owned(),
                    interval_seconds: 43_200,
                    enabled: true,
                },
                next_run_at: Some(
                    Utc.with_ymd_and_hms(2026, 10, 22, 19, 44, 10).unwrap() + chrono::Duration::microseconds(123_456)
                ),
            }
        );
    }
}
