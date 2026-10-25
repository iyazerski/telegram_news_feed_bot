use std::future::Future;

use anyhow::Result;
use common::telegram::bot_api::{TelegramBotApi, TelegramError, Update};
use tracing::error;

use crate::use_cases::manage_feed::{Feed, FeedStore};

pub const HELP_TEXT: &str = "Your personal Telegram news feed is ready 📰
Collect posts from public channels in one place, tune your sources, and keep the updates flowing automatically ✨
Open the menu to manage your feed👇";
pub const ADMIN_ONLY_TEXT: &str = "This bot is restricted to its configured admin.";

/// Where command replies go.
pub trait BotMessenger: Send + Sync {
    fn send_text_message(&self, chat_id: &str, text: &str) -> impl Future<Output = Result<(), TelegramError>> + Send;
}

impl BotMessenger for TelegramBotApi {
    async fn send_text_message(&self, chat_id: &str, text: &str) -> Result<(), TelegramError> {
        TelegramBotApi::send_text_message(self, chat_id, text).await
    }
}

/// Answers the bot's text commands, which only the admin may use.
pub struct CommandHandler<S, M> {
    feed: Feed<S>,
    messenger: M,
    admin_user_id: i64,
}

impl<S: FeedStore, M: BotMessenger> CommandHandler<S, M> {
    pub fn new(feed: Feed<S>, messenger: M, admin_user_id: i64) -> Self {
        Self { feed, messenger, admin_user_id }
    }

    /// Handle one update, and fail only when the command could not be carried out, so that Telegram may resend it.
    pub async fn handle_update(&self, update: &Update) -> Result<()> {
        let Some(message) = &update.message else {
            return Ok(());
        };
        let (Some(text), Some(sender)) = (&message.text, &message.from) else {
            return Ok(());
        };
        let chat_id = message.chat.id;
        if sender.id != self.admin_user_id {
            self.reply(chat_id, ADMIN_ONLY_TEXT).await;
            return Ok(());
        }

        // Commands in groups carry the bot name, as in `/start@NewsFeedBot`.
        let command = text.split_whitespace().next().unwrap_or_default();
        let command = command.split('@').next().unwrap_or_default().to_lowercase();
        if command == "/start" {
            self.feed.connect_chat(chat_id).await?;
            self.reply(chat_id, HELP_TEXT).await;
        }
        Ok(())
    }

    /// Send a reply, logging a failure rather than failing the command that already took effect.
    async fn reply(&self, chat_id: i64, text: &str) {
        if let Err(send_error) = self.messenger.send_text_message(&chat_id.to_string(), text).await {
            error!("Could not answer chat {chat_id}: {send_error}");
        }
    }
}

#[cfg(test)]
pub mod tests {
    use std::sync::{Arc, Mutex};

    use common::telegram::bot_api::{Chat, Message, User};

    use super::*;
    use crate::testing::FakeStore;

    pub const ADMIN_ID: i64 = 123;

    pub type SentReplies = Arc<Mutex<Vec<(String, String)>>>;

    /// A messenger that records (chat, text) replies.
    #[derive(Default)]
    pub struct FakeMessenger {
        sent: SentReplies,
    }

    impl BotMessenger for FakeMessenger {
        async fn send_text_message(&self, chat_id: &str, text: &str) -> Result<(), TelegramError> {
            self.sent.lock().unwrap().push((chat_id.to_owned(), text.to_owned()));
            Ok(())
        }
    }

    /// Return a handler over `store` and the replies it sends.
    pub fn handler(store: &Arc<FakeStore>) -> (CommandHandler<FakeStore, FakeMessenger>, SentReplies) {
        let messenger = FakeMessenger::default();
        let sent = Arc::clone(&messenger.sent);
        (CommandHandler::new(Feed::new(Arc::clone(store), 300), messenger, ADMIN_ID), sent)
    }

    pub fn text_update(update_id: i64, sender_id: i64, chat_id: i64, text: &str) -> Update {
        Update {
            update_id,
            message: Some(Message {
                chat: Chat { id: chat_id },
                from: Some(User { id: sender_id }),
                text: Some(text.to_owned()),
            }),
        }
    }

    #[tokio::test]
    async fn start_connects_the_chat_and_introduces_the_bot() {
        let store = Arc::new(FakeStore::default());
        let (commands, sent) = handler(&store);

        commands.handle_update(&text_update(1, ADMIN_ID, 456, " /Start@NewsFeedBot now")).await.unwrap();

        assert_eq!(store.destination().as_deref(), Some("456"));
        assert_eq!(*sent.lock().unwrap(), [("456".to_owned(), HELP_TEXT.to_owned())]);
    }

    #[tokio::test]
    async fn other_text_is_ignored_and_other_users_are_turned_away() {
        let store = Arc::new(FakeStore::default());
        let (commands, sent) = handler(&store);

        commands.handle_update(&text_update(1, ADMIN_ID, 456, "/unknown")).await.unwrap();
        commands.handle_update(&text_update(2, 999, 789, "/start")).await.unwrap();

        assert_eq!(store.destination(), None);
        assert_eq!(*sent.lock().unwrap(), [("789".to_owned(), ADMIN_ONLY_TEXT.to_owned())]);
    }
}
