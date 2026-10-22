use std::future::Future;
use std::time::Duration;

use common::telegram::bot_api::{TelegramBotApi, TelegramError, Update};
use tracing::{error, info, warn};

use crate::entrypoints::commands::{BotMessenger, CommandHandler};
use crate::use_cases::manage_feed::FeedStore;

/// Where long-polled updates come from.
pub trait UpdateSource: Send + Sync {
    fn get_updates(
        &self,
        offset: i64,
        wait: Duration,
    ) -> impl Future<Output = Result<Vec<Update>, TelegramError>> + Send;
}

impl UpdateSource for TelegramBotApi {
    async fn get_updates(&self, offset: i64, wait: Duration) -> Result<Vec<Update>, TelegramError> {
        TelegramBotApi::get_updates(self, offset, wait).await
    }
}

/// Receives updates by long polling, for local development without a public webhook URL.
pub struct UpdatePoller<S, M, U> {
    commands: CommandHandler<S, M>,
    source: U,
    wait: Duration,
    retry_delay: Duration,
}

impl<S: FeedStore, M: BotMessenger, U: UpdateSource> UpdatePoller<S, M, U> {
    pub fn new(commands: CommandHandler<S, M>, source: U, wait: Duration, retry_delay: Duration) -> Self {
        Self { commands, source, wait, retry_delay }
    }

    /// Poll and handle updates until the future is dropped.
    pub async fn run_forever(&self) {
        info!("Polling Telegram for updates");
        let mut offset = 0;
        loop {
            offset = self.poll_once(offset).await;
        }
    }

    /// Handle one batch of updates from `offset` on, and return the offset of the next batch.
    async fn poll_once(&self, mut offset: i64) -> i64 {
        let updates = match self.source.get_updates(offset, self.wait).await {
            Ok(updates) => updates,
            Err(poll_error) => {
                warn!("Could not get Telegram updates; retrying: {poll_error}");
                tokio::time::sleep(self.retry_delay).await;
                return offset;
            }
        };
        // Long polling already waits for new updates, so the next request starts right away.
        for update in updates {
            offset = update.update_id + 1;
            // A failed update is skipped, so that it cannot block the ones after it.
            if let Err(update_error) = self.commands.handle_update(&update).await {
                error!("Could not handle Telegram update {}: {update_error:#}", update.update_id);
            }
        }
        offset
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::entrypoints::commands::HELP_TEXT;
    use crate::entrypoints::commands::tests::{ADMIN_ID, handler, text_update};
    use crate::testing::FakeStore;

    /// A source that returns its batches in order and records the requested offsets.
    struct FakeSource {
        batches: Mutex<Vec<Result<Vec<Update>, TelegramError>>>,
        offsets: Mutex<Vec<i64>>,
    }

    impl UpdateSource for FakeSource {
        async fn get_updates(&self, offset: i64, _wait: Duration) -> Result<Vec<Update>, TelegramError> {
            self.offsets.lock().unwrap().push(offset);
            self.batches.lock().unwrap().remove(0)
        }
    }

    #[tokio::test]
    async fn a_failed_update_is_skipped_and_a_failed_poll_is_retried_from_the_same_offset() {
        let unavailable = TelegramError::Http { method: "getUpdates".to_owned(), message: "timed out".to_owned() };
        let source = FakeSource {
            batches: Mutex::new(vec![
                Ok(vec![text_update(10, ADMIN_ID, 1, "/start"), text_update(11, ADMIN_ID, 2, "/start")]),
                Err(unavailable),
            ]),
            offsets: Mutex::default(),
        };
        let store = Arc::new(FakeStore::default());
        // The first /start fails as during a database outage.
        store.fail_next_destination_write();
        let (commands, sent) = handler(&store);
        let poller = UpdatePoller::new(commands, source, Duration::ZERO, Duration::ZERO);

        let offset = poller.poll_once(0).await;
        let retried_offset = poller.poll_once(offset).await;

        assert_eq!((offset, retried_offset), (12, 12));
        assert_eq!(*poller.source.offsets.lock().unwrap(), [0, 12]);
        assert_eq!(store.destination().as_deref(), Some("2"));
        assert_eq!(*sent.lock().unwrap(), [("2".to_owned(), HELP_TEXT.to_owned())]);
    }
}
