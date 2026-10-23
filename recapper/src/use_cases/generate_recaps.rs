use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use common::settings::{RecapSchedule, RecapSettings};
use common::telegram::bot_api::{TelegramBotApi, TelegramError};
use tracing::{error, info, warn};

use crate::config::Configs;
use crate::domain::recaps::{RecapStory, select_new_stories};
use crate::infrastructure::ai::prompt::{build_news_recap_prompt, parse_news_recap_output};
use crate::infrastructure::ai::researcher::ResearchError;
use crate::infrastructure::telegram::recap_formatter::build_recap_message;

// Short intervals would only see a few hours of news, so research always covers at least a day.
// Story history keeps longer windows from repeating what earlier recaps sent.
const MIN_RECAP_WINDOW: Duration = Duration::hours(24);
pub const AUTH_ALERT_TEXT: &str = "⚠️ AI news recaps are paused because the recapper is not signed in to ChatGPT. \
     Sign in again with: kubectl exec -it deploy/telegram-news-recapper -- recapper login";

/// The stored recap configuration, schedule, and destination.
pub struct StoredRecap {
    pub schedule: RecapSchedule,
    pub destination: Option<String>,
}

/// Where recap settings, schedule, and history are kept.
pub trait RecapStore {
    /// Read the recap settings, next run time, and destination chat.
    async fn load_recap(&self, default_interval_seconds: i64) -> Result<StoredRecap>;
    /// Return the newest sent stories as (title, url) pairs.
    async fn load_sent_stories(&self, limit: i64) -> Result<Vec<(String, String)>>;
    /// Remember sent stories, prune history older than `prune_before`, and schedule the next recap, atomically.
    async fn commit_recap(
        &self,
        stories: &[RecapStory],
        sent_at: DateTime<Utc>,
        prune_before: DateTime<Utc>,
        next_run_at: DateTime<Utc>,
    ) -> Result<()>;
    /// Store when the next recap attempt runs.
    async fn schedule(&self, run_at: DateTime<Utc>) -> Result<()>;
}

/// What researches a recap prompt.
pub trait RecapResearcher {
    /// Run the recap prompt through the agent and return its final reply.
    async fn research(&self, prompt: &str) -> Result<String, ResearchError>;
    /// Return whether a usable sign-in is kept, treating transport failures as still signed in.
    async fn is_signed_in(&self) -> bool;
}

/// Where recap messages go.
pub trait RecapMessenger {
    async fn send_html_message(&self, chat_id: &str, text: &str, disable_preview: bool) -> Result<(), TelegramError>;
    async fn send_text_message(&self, chat_id: &str, text: &str) -> Result<(), TelegramError>;
}

impl RecapMessenger for TelegramBotApi {
    async fn send_html_message(&self, chat_id: &str, text: &str, disable_preview: bool) -> Result<(), TelegramError> {
        TelegramBotApi::send_html_message(self, chat_id, text, disable_preview).await
    }

    async fn send_text_message(&self, chat_id: &str, text: &str) -> Result<(), TelegramError> {
        TelegramBotApi::send_text_message(self, chat_id, text).await
    }
}

/// A recap that is on, due, and has a destination chat.
struct DueRecap {
    settings: RecapSettings,
    destination: String,
    /// Newest first, as (title, url) pairs.
    sent_stories: Vec<(String, String)>,
}

/// The recap limits the use case needs from the service settings.
pub struct RecapPolicy {
    pub default_interval_seconds: i64,
    pub retry_delay: Duration,
    pub story_count: usize,
    pub history_size: i64,
    pub history_retention: Duration,
}

impl RecapPolicy {
    /// Read the recap limits from the service settings.
    pub fn from_configs(configs: &Configs) -> Self {
        Self {
            default_interval_seconds: configs.default_recap_interval_seconds,
            retry_delay: Duration::seconds(configs.recap_retry_delay_seconds),
            story_count: configs.recap_story_count,
            history_size: configs.recap_history_size,
            history_retention: Duration::days(configs.recap_history_retention_days),
        }
    }
}

/// Researches, formats, and sends AI news recaps on their schedule.
pub struct NewsRecapService<S, R, M> {
    store: S,
    researcher: R,
    messenger: M,
    policy: RecapPolicy,
}

impl<S: RecapStore, R: RecapResearcher, M: RecapMessenger> NewsRecapService<S, R, M> {
    /// Create the recap use case over its storage, research, and messaging boundaries.
    pub fn new(store: S, researcher: R, messenger: M, policy: RecapPolicy) -> Self {
        Self { store, researcher, messenger, policy }
    }

    /// Generate and send one recap when the schedule says it is due.
    pub async fn run_if_due(&self, now: DateTime<Utc>) -> Result<()> {
        let Some(due) = self.load_due_recap(now).await? else {
            return Ok(());
        };

        let interval = Duration::seconds(due.settings.interval_seconds);
        let prompt = build_news_recap_prompt(
            &due.settings,
            now,
            now - interval.max(MIN_RECAP_WINDOW),
            self.policy.story_count,
            &due.sent_stories,
        );
        let stories = match self.researcher.research(&prompt).await {
            Ok(output) => parse_news_recap_output(&output),
            Err(research_error) => Err(research_error.to_string()),
        };
        let stories = match stories {
            Ok(stories) => stories,
            Err(generation_error) => return self.handle_generation_failure(&due, now, &generation_error).await,
        };

        let sent_urls = due.sent_stories.iter().map(|(_, url)| url.as_str());
        let stories = select_new_stories(stories, sent_urls, self.policy.story_count);
        let message = build_recap_message(&stories, &due.settings.topics);
        if let Err(delivery_error) = self.messenger.send_html_message(&due.destination, &message, true).await {
            if delivery_error.is_retryable() {
                warn!("Recap delivery failed; retrying later: {delivery_error}");
                return self.store.schedule(now + self.policy.retry_delay).await;
            }
            // A rejected message would be rejected again, so wait for the next regular recap.
            error!("Telegram rejected the recap: {delivery_error}");
            return self.store.schedule(now + interval).await;
        }

        self.store.commit_recap(&stories, now, now - self.policy.history_retention, now + interval).await?;
        info!("Sent a recap with {} stories", stories.len());
        Ok(())
    }

    /// Read the recap inputs when recaps are on, due, and have a destination chat.
    async fn load_due_recap(&self, now: DateTime<Utc>) -> Result<Option<DueRecap>> {
        let StoredRecap { schedule: RecapSchedule { settings, next_run_at }, destination } =
            self.store.load_recap(self.policy.default_interval_seconds).await?;
        let due = next_run_at.is_some_and(|run_at| run_at <= now);
        if !settings.enabled || settings.topics.is_empty() || !due {
            return Ok(None);
        }
        let Some(destination) = destination else {
            return Ok(None);
        };
        let sent_stories = self.store.load_sent_stories(self.policy.history_size).await?;
        Ok(Some(DueRecap { settings, destination, sent_stories }))
    }

    /// Retry soon after transient failures, and alert once per interval when the ChatGPT sign-in is lost.
    async fn handle_generation_failure(&self, due: &DueRecap, now: DateTime<Utc>, failure: &str) -> Result<()> {
        if self.researcher.is_signed_in().await {
            warn!("Recap generation failed; retrying later: {failure}");
            return self.store.schedule(now + self.policy.retry_delay).await;
        }

        error!("Recap generation failed because the recapper is not signed in to ChatGPT: {failure}");
        if let Err(alert_error) = self.messenger.send_text_message(&due.destination, AUTH_ALERT_TEXT).await {
            error!("Could not send the sign-in alert: {alert_error}");
        }
        self.store.schedule(now + Duration::seconds(due.settings.interval_seconds)).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use chrono::TimeZone;

    use super::*;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 20, 18, 0, 0).unwrap()
    }

    const INTERVAL: Duration = Duration::hours(6);

    /// Create a recap reply with one story per URL.
    fn build_output(urls: &[&str]) -> String {
        let stories: Vec<String> = urls
            .iter()
            .enumerate()
            .map(|(index, url)| {
                format!(
                    r#"{{"emoji": "📰", "title": "Story {index}", "summary": "Summary.", "source_name": "Source", "url": "{url}"}}"#
                )
            })
            .collect();
        format!(r#"{{"stories": [{}]}}"#, stories.join(", "))
    }

    /// An in-memory store with recaps on, due now, and one previously sent story.
    struct FakeStore {
        next_run_at: Mutex<Option<DateTime<Utc>>>,
        sent: Mutex<Vec<(String, String, DateTime<Utc>)>>,
    }

    impl FakeStore {
        fn due_now() -> Self {
            Self {
                next_run_at: Mutex::new(Some(now())),
                sent: Mutex::new(vec![
                    ("Old".to_owned(), "https://news.example/old".to_owned(), now() - Duration::days(1)),
                    ("Stale".to_owned(), "https://news.example/stale".to_owned(), now() - Duration::days(60)),
                ]),
            }
        }

        fn next_run_at(&self) -> Option<DateTime<Utc>> {
            *self.next_run_at.lock().unwrap()
        }

        fn sent_urls(&self) -> Vec<String> {
            self.sent.lock().unwrap().iter().map(|(_, url, _)| url.clone()).collect()
        }
    }

    impl RecapStore for FakeStore {
        async fn load_recap(&self, _default_interval_seconds: i64) -> Result<StoredRecap> {
            Ok(StoredRecap {
                schedule: RecapSchedule {
                    settings: RecapSettings {
                        topics: vec!["AI".to_owned()],
                        language: "English".to_owned(),
                        interval_seconds: INTERVAL.num_seconds(),
                        enabled: true,
                    },
                    next_run_at: self.next_run_at(),
                },
                destination: Some("@dest".to_owned()),
            })
        }

        async fn load_sent_stories(&self, limit: i64) -> Result<Vec<(String, String)>> {
            let mut sent = self.sent.lock().unwrap().clone();
            sent.sort_by_key(|(_, _, sent_at)| std::cmp::Reverse(*sent_at));
            Ok(sent.into_iter().take(limit as usize).map(|(title, url, _)| (title, url)).collect())
        }

        async fn commit_recap(
            &self,
            stories: &[RecapStory],
            sent_at: DateTime<Utc>,
            prune_before: DateTime<Utc>,
            next_run_at: DateTime<Utc>,
        ) -> Result<()> {
            let mut sent = self.sent.lock().unwrap();
            sent.extend(stories.iter().map(|story| (story.title.clone(), story.url.clone(), sent_at)));
            sent.retain(|(_, _, sent_at)| *sent_at >= prune_before);
            *self.next_run_at.lock().unwrap() = Some(next_run_at);
            Ok(())
        }

        async fn schedule(&self, run_at: DateTime<Utc>) -> Result<()> {
            *self.next_run_at.lock().unwrap() = Some(run_at);
            Ok(())
        }
    }

    /// A researcher with a fixed reply and sign-in state that records its prompts.
    struct FakeResearcher {
        output: Result<String, String>,
        signed_in: bool,
        prompts: Mutex<Vec<String>>,
    }

    impl FakeResearcher {
        fn replying(output: Result<String, String>) -> Self {
            Self { output, signed_in: true, prompts: Mutex::default() }
        }
    }

    impl RecapResearcher for FakeResearcher {
        async fn research(&self, prompt: &str) -> Result<String, ResearchError> {
            self.prompts.lock().unwrap().push(prompt.to_owned());
            self.output.clone().map_err(ResearchError::Agent)
        }

        async fn is_signed_in(&self) -> bool {
            self.signed_in
        }
    }

    /// A messenger that records messages, or fails HTML sends with the given error.
    #[derive(Default)]
    struct FakeMessenger {
        html_error: Option<fn() -> TelegramError>,
        html_messages: Mutex<Vec<(String, String, bool)>>,
        text_messages: Mutex<Vec<(String, String)>>,
    }

    impl RecapMessenger for FakeMessenger {
        async fn send_html_message(
            &self,
            chat_id: &str,
            text: &str,
            disable_preview: bool,
        ) -> Result<(), TelegramError> {
            if let Some(error) = self.html_error {
                return Err(error());
            }
            self.html_messages.lock().unwrap().push((chat_id.to_owned(), text.to_owned(), disable_preview));
            Ok(())
        }

        async fn send_text_message(&self, chat_id: &str, text: &str) -> Result<(), TelegramError> {
            self.text_messages.lock().unwrap().push((chat_id.to_owned(), text.to_owned()));
            Ok(())
        }
    }

    fn policy() -> RecapPolicy {
        RecapPolicy {
            default_interval_seconds: 21_600,
            retry_delay: Duration::minutes(15),
            story_count: 5,
            history_size: 50,
            history_retention: Duration::days(30),
        }
    }

    fn service(
        researcher: FakeResearcher,
        messenger: FakeMessenger,
    ) -> NewsRecapService<FakeStore, FakeResearcher, FakeMessenger> {
        NewsRecapService::new(FakeStore::due_now(), researcher, messenger, policy())
    }

    #[tokio::test]
    async fn due_recap_is_sent_once_without_repeating_stories() {
        // New stories go out without previews, are remembered, old history is pruned, and the next recap is scheduled.
        let output = build_output(&["https://news.example/old", "https://news.example/new"]);
        let recaps = service(FakeResearcher::replying(Ok(output)), FakeMessenger::default());

        recaps.run_if_due(now()).await.unwrap();
        recaps.run_if_due(now()).await.unwrap();

        let prompts = recaps.researcher.prompts.lock().unwrap();
        assert_eq!(prompts.len(), 1);
        assert!(prompts[0].contains("since 2026-10-19 18:00 UTC"));
        assert!(prompts[0].contains("- Old (https://news.example/old)"));
        let html_messages = recaps.messenger.html_messages.lock().unwrap();
        let [(chat_id, message, disable_preview)] = html_messages.as_slice() else {
            panic!("expected one recap message, got {html_messages:?}");
        };
        assert_eq!((chat_id.as_str(), *disable_preview), ("@dest", true));
        assert!(message.contains("https://news.example/new"));
        assert!(!message.contains("https://news.example/old"));
        assert_eq!(recaps.store.sent_urls(), ["https://news.example/old", "https://news.example/new"]);
        assert_eq!(recaps.store.next_run_at(), Some(now() + INTERVAL));
    }

    #[tokio::test]
    async fn failed_generation_retries_soon_while_signed_in() {
        // Agent failures and unusable replies both retry after the retry delay without messaging anyone.
        for output in [Err("rate limited".to_owned()), Ok("not json".to_owned())] {
            let recaps = service(FakeResearcher::replying(output), FakeMessenger::default());

            recaps.run_if_due(now()).await.unwrap();

            assert!(recaps.messenger.html_messages.lock().unwrap().is_empty());
            assert!(recaps.messenger.text_messages.lock().unwrap().is_empty());
            assert_eq!(recaps.store.next_run_at(), Some(now() + Duration::minutes(15)));
        }
    }

    #[tokio::test]
    async fn lost_sign_in_alerts_once_per_interval() {
        // The user is told to sign in again, and the next attempt waits a full interval.
        let researcher =
            FakeResearcher { signed_in: false, ..FakeResearcher::replying(Err("unauthorized".to_owned())) };
        let recaps = service(researcher, FakeMessenger::default());

        recaps.run_if_due(now()).await.unwrap();

        assert_eq!(*recaps.messenger.text_messages.lock().unwrap(), [("@dest".to_owned(), AUTH_ALERT_TEXT.to_owned())]);
        assert_eq!(recaps.store.next_run_at(), Some(now() + INTERVAL));
    }

    #[tokio::test]
    async fn delivery_failures_reschedule_without_remembering_stories() {
        // Transient Telegram failures retry soon, and a rejection waits for the next interval.
        let cases: [(fn() -> TelegramError, Duration); 3] = [
            (
                || TelegramError::Api {
                    method: "sendMessage".to_owned(),
                    description: "Too Many Requests".to_owned(),
                    error_code: 429,
                },
                Duration::minutes(15),
            ),
            (
                || TelegramError::Http { method: "sendMessage".to_owned(), message: "offline".to_owned() },
                Duration::minutes(15),
            ),
            (
                || TelegramError::Api {
                    method: "sendMessage".to_owned(),
                    description: "Bad Request".to_owned(),
                    error_code: 400,
                },
                INTERVAL,
            ),
        ];
        for (error, delay) in cases {
            let output = build_output(&["https://news.example/new"]);
            let messenger = FakeMessenger { html_error: Some(error), ..FakeMessenger::default() };
            let recaps = service(FakeResearcher::replying(Ok(output)), messenger);

            recaps.run_if_due(now()).await.unwrap();

            assert_eq!(recaps.store.next_run_at(), Some(now() + delay));
            assert!(!recaps.store.sent_urls().contains(&"https://news.example/new".to_owned()));
        }
    }

    #[tokio::test]
    async fn recap_waits_until_due() {
        let recaps = service(FakeResearcher::replying(Ok(build_output(&[]))), FakeMessenger::default());

        recaps.run_if_due(now() - Duration::minutes(1)).await.unwrap();

        assert!(recaps.researcher.prompts.lock().unwrap().is_empty());
    }
}
