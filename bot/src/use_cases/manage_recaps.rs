use std::future::Future;
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use common::settings::{RecapSchedule, RecapSettings};

use crate::use_cases::ManageError;

/// Where the recap settings and schedule are kept.
pub trait RecapStore: Send + Sync {
    fn load_recap(&self, default_interval_seconds: i64) -> impl Future<Output = Result<RecapSchedule>> + Send;
    /// Store the settings and, when given, the next run time, atomically.
    fn store_recap(
        &self,
        settings: &RecapSettings,
        next_run_at: Option<DateTime<Utc>>,
    ) -> impl Future<Output = Result<()>> + Send;
    fn store_recap_next_run_at(&self, run_at: DateTime<Utc>) -> impl Future<Output = Result<()>> + Send;
}

/// Manages the AI recap settings and when the recapper sends the next recap.
pub struct RecapService<S> {
    store: Arc<S>,
    default_interval_seconds: i64,
}

impl<S: RecapStore> RecapService<S> {
    /// Create the use case over the shared store.
    pub fn new(store: Arc<S>, default_interval_seconds: i64) -> Self {
        Self { store, default_interval_seconds }
    }

    /// Return the recap settings and the next run time, which is none while recaps are off.
    pub async fn schedule(&self) -> Result<RecapSchedule> {
        let schedule = self.store.load_recap(self.default_interval_seconds).await?;
        Ok(visible(schedule))
    }

    /// Store new settings and reschedule, so that a new setup or a shorter interval applies right away.
    pub async fn save(&self, settings: RecapSettings, now: DateTime<Utc>) -> Result<RecapSchedule, ManageError> {
        if settings.enabled && settings.topics.is_empty() {
            return Err(ManageError::Rejected("Add at least one topic before turning AI recaps on.".to_owned()));
        }
        let previous = self.store.load_recap(self.default_interval_seconds).await?;
        // Turning recaps on sends the first one now, and a shorter interval never delays the next one. Turning them
        // off keeps the stored time, which nothing reads while they are off.
        let next_run_at = settings.enabled.then(|| match previous.next_run_at {
            Some(run_at) if previous.settings.enabled => run_at.min(now + Duration::seconds(settings.interval_seconds)),
            _ => now,
        });
        self.store.store_recap(&settings, next_run_at).await?;
        Ok(RecapSchedule { settings, next_run_at })
    }

    /// Make the recapper send a recap on its next check.
    pub async fn request_now(&self, now: DateTime<Utc>) -> Result<RecapSchedule, ManageError> {
        let schedule = self.store.load_recap(self.default_interval_seconds).await?;
        if !schedule.settings.enabled {
            return Err(ManageError::Rejected("Turn AI recaps on before requesting one.".to_owned()));
        }
        self.store.store_recap_next_run_at(now).await?;
        Ok(RecapSchedule { next_run_at: Some(now), ..schedule })
    }
}

/// Hide the stored run time while recaps are off.
fn visible(schedule: RecapSchedule) -> RecapSchedule {
    if schedule.settings.enabled { schedule } else { RecapSchedule { next_run_at: None, ..schedule } }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::testing::FakeStore;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 22, 12, 0, 0).unwrap()
    }

    fn settings(enabled: bool, interval_hours: i64) -> RecapSettings {
        RecapSettings {
            topics: vec!["AI".to_owned()],
            language: "English".to_owned(),
            interval_seconds: interval_hours * 3_600,
            enabled,
        }
    }

    fn service(store: &Arc<FakeStore>) -> RecapService<FakeStore> {
        RecapService::new(Arc::clone(store), 21_600)
    }

    #[tokio::test]
    async fn turning_recaps_on_runs_now_and_a_shorter_interval_never_delays_the_next_run() {
        let store = Arc::new(FakeStore::default());
        let recaps = service(&store);

        let turned_on = recaps.save(settings(true, 24), now()).await.unwrap();
        store.set_next_run_at(Some(now() + Duration::hours(20)));
        let shortened = recaps.save(settings(true, 6), now()).await.unwrap();
        let lengthened = recaps.save(settings(true, 12), now()).await.unwrap();

        assert_eq!(turned_on.next_run_at, Some(now()));
        assert_eq!(shortened.next_run_at, Some(now() + Duration::hours(6)));
        assert_eq!(lengthened.next_run_at, Some(now() + Duration::hours(6)));
        assert_eq!(store.next_run_at(), Some(now() + Duration::hours(6)));
    }

    #[tokio::test]
    async fn turning_recaps_off_hides_and_keeps_the_stored_run_time() {
        let store = Arc::new(FakeStore::default());
        let recaps = service(&store);
        recaps.save(settings(true, 6), now()).await.unwrap();

        let turned_off = recaps.save(settings(false, 6), now() + Duration::hours(1)).await.unwrap();

        assert_eq!(turned_off.next_run_at, None);
        assert_eq!(recaps.schedule().await.unwrap().next_run_at, None);
        assert_eq!(store.next_run_at(), Some(now()));
        // Turning them on again runs a recap now, not at the stale time.
        let later = now() + Duration::hours(2);
        assert_eq!(recaps.save(settings(true, 6), later).await.unwrap().next_run_at, Some(later));
    }

    #[tokio::test]
    async fn recaps_without_topics_or_turned_off_are_rejected() {
        let store = Arc::new(FakeStore::default());
        let recaps = service(&store);
        let without_topics = RecapSettings { topics: Vec::new(), ..settings(true, 6) };

        let save_error = recaps.save(without_topics, now()).await.unwrap_err();
        let request_error = recaps.request_now(now()).await.unwrap_err();
        recaps.save(settings(true, 6), now() - Duration::hours(1)).await.unwrap();
        let requested = recaps.request_now(now()).await.unwrap();

        assert!(matches!(save_error, ManageError::Rejected(_)));
        assert!(matches!(request_error, ManageError::Rejected(_)));
        assert_eq!(requested.next_run_at, Some(now()));
        assert_eq!(store.next_run_at(), Some(now()));
    }
}
