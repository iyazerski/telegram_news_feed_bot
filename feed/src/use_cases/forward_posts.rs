use std::time::Duration;

use anyhow::Result;
use common::telegram::bot_api::{TelegramBotApi, TelegramError, TelegramUpload};
use futures::future::join_all;
use tracing::{error, info, warn};

use crate::domain::posts::{DiscoveredPost, SourceChannel};
use crate::infrastructure::database::{FeedSettings, PostgresFeedStore};
use crate::infrastructure::telegram::media_downloader::{MediaError, TelegramMediaDownloader};
use crate::infrastructure::telegram::message_formatter::{
    build_body_messages, build_media_caption, build_message_text, build_text_messages,
};
use crate::infrastructure::telegram::web_preview_client::{PreviewUnavailableError, TelegramWebPreviewClient};
use crate::infrastructure::telegram::web_preview_parser::parse_channel_preview;

// Channels per database page. Each page's previews are fetched together, which also caps concurrent requests.
const CHANNEL_PAGE_SIZE: i64 = 10;

/// Where subscriptions, cursors, and feed settings are kept.
pub trait FeedStore {
    async fn load_settings(&self, default_poll_interval_seconds: u64) -> Result<FeedSettings>;
    async fn list_active_channels(&self, after: &str, limit: i64) -> Result<Vec<SourceChannel>>;
    async fn commit_post(&self, source_channel: &str, message_id: i32) -> Result<bool>;
}

/// Where channel web preview pages come from.
pub trait PreviewSource {
    async fn fetch_channel_preview(&self, username: &str) -> Result<String, PreviewUnavailableError>;
}

/// Where post media comes from.
pub trait MediaSource {
    async fn download_media(&self, media_urls: &[String]) -> Result<Vec<TelegramUpload>, MediaError>;
}

/// Where reposts go.
pub trait FeedMessenger {
    async fn send_html_message(&self, chat_id: &str, text: &str) -> Result<(), TelegramError>;
    async fn send_photo(&self, chat_id: &str, photo: TelegramUpload, caption: &str) -> Result<(), TelegramError>;
    async fn send_video(&self, chat_id: &str, video: TelegramUpload, caption: &str) -> Result<(), TelegramError>;
    async fn send_media_group(
        &self,
        chat_id: &str,
        media: Vec<TelegramUpload>,
        caption: &str,
    ) -> Result<(), TelegramError>;
}

impl FeedStore for PostgresFeedStore {
    async fn load_settings(&self, default_poll_interval_seconds: u64) -> Result<FeedSettings> {
        PostgresFeedStore::load_settings(self, default_poll_interval_seconds).await
    }

    async fn list_active_channels(&self, after: &str, limit: i64) -> Result<Vec<SourceChannel>> {
        PostgresFeedStore::list_active_channels(self, after, limit).await
    }

    async fn commit_post(&self, source_channel: &str, message_id: i32) -> Result<bool> {
        PostgresFeedStore::commit_post(self, source_channel, message_id).await
    }
}

impl PreviewSource for TelegramWebPreviewClient {
    async fn fetch_channel_preview(&self, username: &str) -> Result<String, PreviewUnavailableError> {
        TelegramWebPreviewClient::fetch_channel_preview(self, username).await
    }
}

impl MediaSource for TelegramMediaDownloader {
    async fn download_media(&self, media_urls: &[String]) -> Result<Vec<TelegramUpload>, MediaError> {
        TelegramMediaDownloader::download_media(self, media_urls).await
    }
}

impl FeedMessenger for TelegramBotApi {
    async fn send_html_message(&self, chat_id: &str, text: &str) -> Result<(), TelegramError> {
        TelegramBotApi::send_html_message(self, chat_id, text, false).await
    }

    async fn send_photo(&self, chat_id: &str, photo: TelegramUpload, caption: &str) -> Result<(), TelegramError> {
        TelegramBotApi::send_photo(self, chat_id, photo, caption).await
    }

    async fn send_video(&self, chat_id: &str, video: TelegramUpload, caption: &str) -> Result<(), TelegramError> {
        TelegramBotApi::send_video(self, chat_id, video, caption).await
    }

    async fn send_media_group(
        &self,
        chat_id: &str,
        media: Vec<TelegramUpload>,
        caption: &str,
    ) -> Result<(), TelegramError> {
        TelegramBotApi::send_media_group(self, chat_id, media, caption).await
    }
}

/// Finds new posts on subscribed channels' web previews and reposts them to the bot chat in order.
pub struct PostForwardingService<S, P, D, M> {
    store: S,
    previews: P,
    media: D,
    messenger: M,
    default_poll_interval_seconds: u64,
}

impl<S: FeedStore, P: PreviewSource, D: MediaSource, M: FeedMessenger> PostForwardingService<S, P, D, M> {
    /// Create the forwarding use case over its storage, preview, media, and messaging boundaries.
    pub fn new(store: S, previews: P, media: D, messenger: M, default_poll_interval_seconds: u64) -> Self {
        Self { store, previews, media, messenger, default_poll_interval_seconds }
    }

    /// Forward the new posts of every active channel once, and return how long to wait before the next cycle.
    pub async fn run_cycle(&self) -> Result<Duration> {
        let settings = self.store.load_settings(self.default_poll_interval_seconds).await?;
        match &settings.destination {
            Some(destination) => self.forward_new_posts(destination).await?,
            // Posts stay undelivered on their channels' pages until there is a chat to send them to.
            None => warn!("No bot chat to forward posts to yet; send /start to the bot"),
        }
        Ok(settings.poll_interval)
    }

    async fn forward_new_posts(&self, destination: &str) -> Result<()> {
        let mut after = String::new();
        loop {
            let channels = self.store.list_active_channels(&after, CHANNEL_PAGE_SIZE).await?;
            // Fetch the whole page before delivering, so slow uploads cannot hold pending preview requests into timeouts.
            let discovered = join_all(channels.iter().map(|channel| self.discover_posts(channel))).await;
            for (channel, posts) in channels.iter().zip(discovered) {
                self.forward_channel_posts(destination, &channel.username, posts).await?;
            }
            match channels.last() {
                Some(last) if channels.len() as i64 == CHANNEL_PAGE_SIZE => after.clone_from(&last.username),
                _ => return Ok(()),
            }
        }
    }

    /// Return a channel's new posts, or none when its preview cannot be read this cycle.
    async fn discover_posts(&self, channel: &SourceChannel) -> Vec<DiscoveredPost> {
        let html = match self.previews.fetch_channel_preview(&channel.username).await {
            Ok(html) => html,
            Err(preview_error) => {
                warn!("{preview_error}; retrying next cycle");
                return Vec::new();
            }
        };
        parse_channel_preview(&channel.username, &html, channel.last_committed_message_id).unwrap_or_else(
            |parse_error| {
                error!("Could not read the preview of @{}; retrying next cycle: {parse_error:#}", channel.username);
                Vec::new()
            },
        )
    }

    /// Deliver a channel's posts oldest first, committing each, until one has to wait for the next cycle.
    async fn forward_channel_posts(
        &self,
        destination: &str,
        source_channel: &str,
        posts: Vec<DiscoveredPost>,
    ) -> Result<()> {
        for post in posts {
            let post_ref = format!("@{source_channel}/{}", post.message_id);
            match self.deliver_post(destination, &post).await {
                Ok(()) => info!("Forwarded {post_ref}"),
                // Later posts wait too, so that the channel's posts arrive in order.
                Err(delivery_error) if delivery_error.is_retryable() => {
                    warn!("Could not forward {post_ref}; retrying next cycle: {delivery_error}");
                    return Ok(());
                }
                // Telegram would reject the post again, so it is committed and skipped.
                Err(delivery_error) => error!("Telegram rejected {post_ref}: {delivery_error}"),
            }
            if !self.store.commit_post(source_channel, post.message_id).await? {
                info!("Stopped after {post_ref}: the channel is no longer active");
                return Ok(());
            }
        }
        Ok(())
    }

    /// Deliver one post as text messages, or as a photo, video, or album followed by any text the caption lacks.
    async fn deliver_post(&self, destination: &str, post: &DiscoveredPost) -> Result<(), TelegramError> {
        let media = match self.media.download_media(&post.media_urls).await {
            Ok(media) => media,
            Err(media_error) => {
                warn!(
                    "Forwarding @{}/{} with its source link instead of media: {media_error}",
                    post.source_channel, post.message_id
                );
                let post = DiscoveredPost { media_unavailable: true, ..post.clone() };
                return self.send_text_messages(destination, &post).await;
            }
        };
        if media.is_empty() {
            return self.send_text_messages(destination, post).await;
        }

        let caption = build_media_caption(post);
        match <[TelegramUpload; 1]>::try_from(media) {
            Ok([upload]) if upload.is_video() => self.messenger.send_video(destination, upload, &caption).await?,
            Ok([upload]) => self.messenger.send_photo(destination, upload, &caption).await?,
            Err(media) => self.messenger.send_media_group(destination, media, &caption).await?,
        }
        // A caption without the post text was only the channel header, so the body follows on its own.
        if caption != build_message_text(post) {
            for message in build_body_messages(post) {
                self.messenger.send_html_message(destination, &message).await?;
            }
        }
        Ok(())
    }

    async fn send_text_messages(&self, destination: &str, post: &DiscoveredPost) -> Result<(), TelegramError> {
        for message in build_text_messages(post) {
            self.messenger.send_html_message(destination, &message).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    const INTERVAL: Duration = Duration::from_secs(600);

    /// An in-memory store of (username, cursor) channels that records the size of each page it returns.
    struct FakeStore {
        destination: Option<String>,
        channels: Mutex<Vec<(String, i32)>>,
        page_sizes: Mutex<Vec<usize>>,
    }

    impl FakeStore {
        fn with_channels(usernames: &[&str]) -> Self {
            Self {
                destination: Some("@dest".to_owned()),
                channels: Mutex::new(usernames.iter().map(|username| ((*username).to_owned(), 0)).collect()),
                page_sizes: Mutex::default(),
            }
        }

        fn cursor(&self, username: &str) -> i32 {
            self.channels.lock().unwrap().iter().find(|(name, _)| name == username).unwrap().1
        }
    }

    impl FeedStore for FakeStore {
        async fn load_settings(&self, _default_poll_interval_seconds: u64) -> Result<FeedSettings> {
            Ok(FeedSettings { destination: self.destination.clone(), poll_interval: INTERVAL })
        }

        async fn list_active_channels(&self, after: &str, limit: i64) -> Result<Vec<SourceChannel>> {
            let mut channels: Vec<SourceChannel> = self
                .channels
                .lock()
                .unwrap()
                .iter()
                .filter(|(username, _)| username.as_str() > after)
                .map(|(username, cursor)| SourceChannel {
                    username: username.clone(),
                    last_committed_message_id: *cursor,
                })
                .collect();
            channels.sort_by(|left, right| left.username.cmp(&right.username));
            channels.truncate(limit as usize);
            self.page_sizes.lock().unwrap().push(channels.len());
            Ok(channels)
        }

        async fn commit_post(&self, source_channel: &str, message_id: i32) -> Result<bool> {
            let mut channels = self.channels.lock().unwrap();
            let Some((_, cursor)) = channels.iter_mut().find(|(username, _)| username == source_channel) else {
                return Ok(false);
            };
            *cursor = (*cursor).max(message_id);
            Ok(true)
        }
    }

    /// Preview pages by username; a missing page is unavailable. Records requested usernames.
    #[derive(Default)]
    struct FakePreviews {
        pages: HashMap<String, String>,
        requested: Mutex<Vec<String>>,
    }

    impl PreviewSource for FakePreviews {
        async fn fetch_channel_preview(&self, username: &str) -> Result<String, PreviewUnavailableError> {
            self.requested.lock().unwrap().push(username.to_owned());
            self.pages
                .get(username)
                .cloned()
                .ok_or_else(|| PreviewUnavailableError { username: username.to_owned(), reason: "offline".to_owned() })
        }
    }

    /// Media that downloads as one upload per URL, a video for \`.mp4\`, or fails when \`fail\` is set.
    #[derive(Default)]
    struct FakeMedia {
        fail: bool,
    }

    impl MediaSource for FakeMedia {
        async fn download_media(&self, media_urls: &[String]) -> Result<Vec<TelegramUpload>, MediaError> {
            if self.fail {
                return Err(MediaError::Unsupported("media unavailable".to_owned()));
            }
            Ok(media_urls.iter().enumerate().map(|(index, url)| upload(index, url.ends_with(".mp4"))).collect())
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Sent {
        Html(String),
        Photo(TelegramUpload, String),
        Video(TelegramUpload, String),
        Album(Vec<TelegramUpload>, String),
    }

    /// A messenger that records what it sends to @dest, or fails every send with \`error\`.
    #[derive(Default)]
    struct FakeMessenger {
        error: Option<fn() -> TelegramError>,
        sent: Mutex<Vec<Sent>>,
    }

    impl FakeMessenger {
        fn record(&self, chat_id: &str, sent: Sent) -> Result<(), TelegramError> {
            if let Some(error) = self.error {
                return Err(error());
            }
            assert_eq!(chat_id, "@dest");
            self.sent.lock().unwrap().push(sent);
            Ok(())
        }
    }

    impl FeedMessenger for FakeMessenger {
        async fn send_html_message(&self, chat_id: &str, text: &str) -> Result<(), TelegramError> {
            self.record(chat_id, Sent::Html(text.to_owned()))
        }

        async fn send_photo(&self, chat_id: &str, photo: TelegramUpload, caption: &str) -> Result<(), TelegramError> {
            self.record(chat_id, Sent::Photo(photo, caption.to_owned()))
        }

        async fn send_video(&self, chat_id: &str, video: TelegramUpload, caption: &str) -> Result<(), TelegramError> {
            self.record(chat_id, Sent::Video(video, caption.to_owned()))
        }

        async fn send_media_group(
            &self,
            chat_id: &str,
            media: Vec<TelegramUpload>,
            caption: &str,
        ) -> Result<(), TelegramError> {
            self.record(chat_id, Sent::Album(media, caption.to_owned()))
        }
    }

    type Service = PostForwardingService<FakeStore, FakePreviews, FakeMedia, FakeMessenger>;

    fn service(store: FakeStore, previews: FakePreviews, media: FakeMedia, messenger: FakeMessenger) -> Service {
        PostForwardingService::new(store, previews, media, messenger, 300)
    }

    fn upload(index: usize, is_video: bool) -> TelegramUpload {
        let (kind, extension, content_type) =
            if is_video { ("video", "mp4", "video/mp4") } else { ("photo", "jpg", "image/jpeg") };
        TelegramUpload {
            field_name: format!("media{index}"),
            filename: format!("telegram-media-{index}.{extension}"),
            content: format!("{kind}-{index}").into_bytes(),
            content_type: content_type.to_owned(),
        }
    }

    fn post(message_id: i32, text_html: &str, media_urls: &[&str]) -> DiscoveredPost {
        DiscoveredPost {
            source_channel: "example".to_owned(),
            channel_display_name: "Example News".to_owned(),
            message_id,
            text_html: text_html.to_owned(),
            media_urls: media_urls.iter().map(|url| (*url).to_owned()).collect(),
            post_url: format!("https://t.me/example/{message_id}"),
            media_unavailable: false,
        }
    }

    /// Build a preview page with one text post per message ID.
    fn preview_page(username: &str, message_ids: &[i32]) -> String {
        message_ids
            .iter()
            .map(|id| {
                format!(
                    r#"<div data-post="{username}/{id}"><a class="tgme_widget_message_owner_name">{username}</a>
                    <div class="js-message_text">Post {id}</div></div>"#
                )
            })
            .collect()
    }

    fn rejection() -> TelegramError {
        TelegramError::Api { method: "sendMessage".to_owned(), description: "Bad Request".to_owned(), error_code: 400 }
    }

    fn rate_limit() -> TelegramError {
        TelegramError::Api { method: "sendMessage".to_owned(), description: "Too Many".to_owned(), error_code: 429 }
    }

    async fn forward(service: &Service, posts: Vec<DiscoveredPost>) -> Vec<Sent> {
        service.forward_channel_posts("@dest", "example", posts).await.unwrap();
        std::mem::take(&mut *service.messenger.sent.lock().unwrap())
    }

    fn delivery_service(media: FakeMedia, messenger: FakeMessenger) -> Service {
        service(FakeStore::with_channels(&["example"]), FakePreviews::default(), media, messenger)
    }

    #[tokio::test]
    async fn posts_are_sent_with_the_matching_telegram_method_and_committed() {
        let forwarding = delivery_service(FakeMedia::default(), FakeMessenger::default());
        let caption = |text: &str| format!("<b><u>Example News</u></b>\n\n{text}");

        let text = forward(&forwarding, vec![post(1, r#"Hi <a href="https://example.com">x</a>"#, &[])]).await;
        let photo = forward(&forwarding, vec![post(2, "<b>Photo</b>", &["https://cdn.example/a.jpg"])]).await;
        let video = forward(&forwarding, vec![post(3, "Video", &["https://cdn.example/a.mp4"])]).await;
        let album =
            forward(&forwarding, vec![post(4, "Album", &["https://cdn.example/a.jpg", "https://cdn.example/b.mp4"])])
                .await;

        assert_eq!(text, [Sent::Html(caption(r#"Hi <a href="https://example.com">x</a>"#))]);
        assert_eq!(photo, [Sent::Photo(upload(0, false), caption("<b>Photo</b>"))]);
        assert_eq!(video, [Sent::Video(upload(0, true), caption("Video"))]);
        assert_eq!(album, [Sent::Album(vec![upload(0, false), upload(1, true)], caption("Album"))]);
        assert_eq!(forwarding.store.cursor("example"), 4);
    }

    #[tokio::test]
    async fn unavailable_media_is_replaced_by_the_source_link() {
        // Media the preview does not expose, and media that fails to download, both link to the source post.
        let exposed_none = DiscoveredPost { media_unavailable: true, ..post(1, "Post text", &[]) };
        let failing = delivery_service(FakeMedia { fail: true }, FakeMessenger::default());
        let unexposed = delivery_service(FakeMedia::default(), FakeMessenger::default());

        let failed_download = forward(&failing, vec![post(1, "Post text", &["https://cdn.example/a.jpg"])]).await;
        let unexposed_media = forward(&unexposed, vec![exposed_none]).await;

        let expected = [Sent::Html("<b><u>Example News</u></b>\n\nPost text\n\nhttps://t.me/example/1".to_owned())];
        assert_eq!(failed_download, expected);
        assert_eq!(unexposed_media, expected);
        assert_eq!(failing.store.cursor("example"), 1);
    }

    #[tokio::test]
    async fn a_body_too_long_for_the_caption_follows_the_media() {
        let body = "a".repeat(1020);
        let forwarding = delivery_service(FakeMedia::default(), FakeMessenger::default());

        let sent = forward(&forwarding, vec![post(1, &body, &["https://cdn.example/a.jpg"])]).await;

        assert_eq!(sent, [Sent::Photo(upload(0, false), "<b><u>Example News</u></b>".to_owned()), Sent::Html(body)]);
    }

    #[tokio::test]
    async fn transient_failures_hold_the_channel_and_rejections_skip_the_post() {
        let held = delivery_service(
            FakeMedia::default(),
            FakeMessenger { error: Some(rate_limit), ..FakeMessenger::default() },
        );
        let rejected = delivery_service(
            FakeMedia::default(),
            FakeMessenger { error: Some(rejection), ..FakeMessenger::default() },
        );

        forward(&held, vec![post(1, "One", &[]), post(2, "Two", &[])]).await;
        forward(&rejected, vec![post(1, "One", &[]), post(2, "Two", &[])]).await;

        assert_eq!(held.store.cursor("example"), 0);
        assert_eq!(rejected.store.cursor("example"), 2);
    }

    #[tokio::test]
    async fn a_cycle_pages_through_channels_and_isolates_unreadable_previews() {
        // 23 channels take three pages. One preview is offline and one cannot be parsed; the others still forward.
        let usernames: Vec<String> = (0..23).map(|index| format!("channel{index:02}")).collect();
        let mut pages: HashMap<String, String> =
            usernames.iter().map(|username| (username.clone(), preview_page(username, &[5]))).collect();
        pages.remove("channel03");
        pages.insert("channel04".to_owned(), r#"<div data-post="channel04/9"></div>"#.to_owned());
        let names: Vec<&str> = usernames.iter().map(String::as_str).collect();
        let forwarding = service(
            FakeStore::with_channels(&names),
            FakePreviews { pages, ..FakePreviews::default() },
            FakeMedia::default(),
            FakeMessenger::default(),
        );

        let interval = forwarding.run_cycle().await.unwrap();

        assert_eq!(interval, INTERVAL);
        assert_eq!(*forwarding.store.page_sizes.lock().unwrap(), [10, 10, 3]);
        assert_eq!(*forwarding.previews.requested.lock().unwrap(), usernames);
        assert_eq!(forwarding.messenger.sent.lock().unwrap().len(), 21);
        assert_eq!(forwarding.store.cursor("channel03"), 0);
        assert_eq!(forwarding.store.cursor("channel04"), 0);
        assert_eq!(forwarding.store.cursor("channel22"), 5);
    }

    #[tokio::test]
    async fn a_cycle_without_a_bot_chat_fetches_nothing() {
        let store = FakeStore { destination: None, ..FakeStore::with_channels(&["example"]) };
        let forwarding = service(store, FakePreviews::default(), FakeMedia::default(), FakeMessenger::default());

        assert_eq!(forwarding.run_cycle().await.unwrap(), INTERVAL);
        assert!(forwarding.previews.requested.lock().unwrap().is_empty());
    }
}
