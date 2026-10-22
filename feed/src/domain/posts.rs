/// A subscribed public channel and the newest of its posts already delivered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceChannel {
    pub username: String,
    pub last_committed_message_id: i32,
}

/// One post found on a public channel's web preview page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPost {
    pub source_channel: String,
    pub channel_display_name: String,
    pub message_id: i32,
    /// The post text in the HTML subset that the Bot API accepts.
    pub text_html: String,
    pub media_urls: Vec<String>,
    pub post_url: String,
    /// Whether the post has media that the preview does not expose or that could not be downloaded.
    pub media_unavailable: bool,
}
