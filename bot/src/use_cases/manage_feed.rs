use std::future::Future;
use std::sync::Arc;

use anyhow::Result;

use crate::domain::channels::normalize_channel_username;
use crate::use_cases::ManageError;

/// Channels per Mini App page.
pub const CHANNEL_PAGE_SIZE: usize = 100;

/// Where the bot chat, the poll interval, and channel subscriptions are kept.
pub trait FeedStore: Send + Sync {
    fn store_destination_chat_id(&self, chat_id: &str) -> impl Future<Output = Result<()>> + Send;
    fn load_poll_interval_seconds(&self) -> impl Future<Output = Result<Option<u64>>> + Send;
    fn store_poll_interval_seconds(&self, seconds: u64) -> impl Future<Output = Result<()>> + Send;
    /// Return up to `limit` active channel usernames after `after`, in username order.
    fn list_active_channels(&self, after: &str, limit: usize) -> impl Future<Output = Result<Vec<String>>> + Send;
    /// Subscribe to a channel, or resubscribe and keep its cursor.
    fn activate_channel(&self, username: &str) -> impl Future<Output = Result<()>> + Send;
    /// Unsubscribe from a channel, and return false when it was never added.
    fn deactivate_channel(&self, username: &str) -> impl Future<Output = Result<bool>> + Send;
}

/// One page of active channel usernames, with the cursor of the next page when there is one.
#[derive(Debug, PartialEq, Eq)]
pub struct ChannelPage {
    pub usernames: Vec<String>,
    pub next_cursor: Option<String>,
}

/// Manages where the feed goes, how often it polls, and which channels it follows.
pub struct Feed<S> {
    store: Arc<S>,
    default_poll_interval_seconds: u64,
}

impl<S: FeedStore> Feed<S> {
    /// Create the use case over the shared store.
    pub fn new(store: Arc<S>, default_poll_interval_seconds: u64) -> Self {
        Self { store, default_poll_interval_seconds }
    }

    /// Make `chat_id` the chat that receives posts and recaps.
    pub async fn connect_chat(&self, chat_id: i64) -> Result<()> {
        self.store.store_destination_chat_id(&chat_id.to_string()).await
    }

    /// Return the poll interval, which falls back to the default before the first save.
    pub async fn poll_interval_seconds(&self) -> Result<u64> {
        Ok(self.store.load_poll_interval_seconds().await?.unwrap_or(self.default_poll_interval_seconds))
    }

    pub async fn set_poll_interval_seconds(&self, seconds: u64) -> Result<()> {
        self.store.store_poll_interval_seconds(seconds).await
    }

    /// Return the page of active channels after the `after` username.
    pub async fn channel_page(&self, after: &str) -> Result<ChannelPage> {
        // One extra row tells whether another page follows.
        let mut usernames = self.store.list_active_channels(after, CHANNEL_PAGE_SIZE + 1).await?;
        let has_more = usernames.len() > CHANNEL_PAGE_SIZE;
        usernames.truncate(CHANNEL_PAGE_SIZE);
        let next_cursor = if has_more { usernames.last().cloned() } else { None };
        Ok(ChannelPage { usernames, next_cursor })
    }

    /// Follow the channel given as a username or URL, and return its username.
    pub async fn add_channel(&self, username_or_url: &str) -> Result<String, ManageError> {
        let username = normalize_channel_username(username_or_url).map_err(rejected)?;
        self.store.activate_channel(&username).await?;
        Ok(username)
    }

    /// Stop following the channel given as a username or URL.
    pub async fn remove_channel(&self, username_or_url: &str) -> Result<(), ManageError> {
        let username = normalize_channel_username(username_or_url).map_err(rejected)?;
        if !self.store.deactivate_channel(&username).await? {
            return Err(ManageError::Rejected(format!("Channel @{username} is not configured")));
        }
        Ok(())
    }
}

fn rejected(error: impl ToString) -> ManageError {
    ManageError::Rejected(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeStore;

    fn service(store: &Arc<FakeStore>) -> Feed<FakeStore> {
        Feed::new(Arc::clone(store), 300)
    }

    #[tokio::test]
    async fn channel_pages_cover_active_channels_once_and_end_without_a_cursor() {
        let store = Arc::new(FakeStore::default());
        let usernames: Vec<String> = (0..CHANNEL_PAGE_SIZE + 2).map(|index| format!("channel{index:04}")).collect();
        for username in usernames.iter().rev() {
            store.add_channel(username, true);
        }
        store.add_channel("channel0000inactive", false);
        let feed = service(&store);

        let first = feed.channel_page("").await.unwrap();
        let second = feed.channel_page(first.next_cursor.as_deref().unwrap()).await.unwrap();

        assert_eq!(first.usernames, usernames[..CHANNEL_PAGE_SIZE]);
        assert_eq!(first.next_cursor.as_ref(), usernames.get(CHANNEL_PAGE_SIZE - 1));
        assert_eq!(second, ChannelPage { usernames: usernames[CHANNEL_PAGE_SIZE..].to_vec(), next_cursor: None });
    }

    #[tokio::test]
    async fn channels_are_added_by_url_and_removing_an_unknown_one_is_rejected() {
        let store = Arc::new(FakeStore::default());
        let feed = service(&store);

        assert_eq!(feed.add_channel("https://t.me/Example").await.unwrap(), "example");
        feed.remove_channel("@example").await.unwrap();
        let unknown = feed.remove_channel("missing").await.unwrap_err();
        let invalid = feed.add_channel("https://example.com/news").await.unwrap_err();

        assert!(feed.channel_page("").await.unwrap().usernames.is_empty());
        assert!(matches!(unknown, ManageError::Rejected(message) if message == "Channel @missing is not configured"));
        assert!(matches!(invalid, ManageError::Rejected(_)));
    }
}
