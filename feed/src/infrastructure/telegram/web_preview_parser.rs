use std::sync::LazyLock;

use anyhow::{Context, Result};
use common::telegram::html::{escape_attribute, escape_text};
use regex::Regex;
use scraper::{ElementRef, Html, Node, Selector};

use crate::domain::posts::DiscoveredPost;

static POST: LazyLock<Selector> = LazyLock::new(|| selector("[data-post]"));
static OWNER_NAME: LazyLock<Selector> = LazyLock::new(|| selector(".tgme_widget_message_owner_name"));
static MESSAGE_TEXT: LazyLock<Selector> = LazyLock::new(|| selector(".js-message_text"));
static MEDIA_WRAP: LazyLock<Selector> =
    LazyLock::new(|| selector(".tgme_widget_message_photo_wrap, .tgme_widget_message_video_wrap"));
static VIDEO: LazyLock<Selector> = LazyLock::new(|| selector("video"));
static MEDIA_NOT_SUPPORTED: LazyLock<Selector> = LazyLock::new(|| selector(".message_media_not_supported_wrap"));
static BACKGROUND_IMAGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"background-image:url\(['"]?(?P<url>[^'")]+)"#).expect("the background image pattern is valid")
});

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("the selector is valid")
}

/// Extract the posts of `source_channel` newer than `after_message_id` from its web preview page, oldest first.
pub fn parse_channel_preview(source_channel: &str, html: &str, after_message_id: i32) -> Result<Vec<DiscoveredPost>> {
    let document = Html::parse_document(html);
    let mut posts = Vec::new();
    // Telegram exposes public preview message IDs in data-post attributes.
    for message in document.select(&POST) {
        let Some(data_post) = message.value().attr("data-post") else {
            continue;
        };
        let (channel, raw_message_id) =
            data_post.split_once('/').with_context(|| format!("invalid Telegram data-post {data_post:?}"))?;
        let channel = channel.to_lowercase();
        if channel != source_channel {
            continue;
        }
        let message_id: i32 =
            raw_message_id.parse().with_context(|| format!("invalid Telegram data-post {data_post:?}"))?;
        // Committed posts are skipped before their metadata is read, so they cannot fail the page.
        if message_id <= after_message_id {
            continue;
        }
        let (media_urls, media_unavailable) = extract_media_urls(message);
        posts.push(DiscoveredPost {
            channel_display_name: extract_channel_display_name(message)?,
            text_html: extract_text_html(message),
            media_urls,
            post_url: format!("https://t.me/{channel}/{message_id}"),
            media_unavailable,
            source_channel: channel,
            message_id,
        });
    }
    posts.sort_by_key(|post| post.message_id);
    Ok(posts)
}

fn extract_channel_display_name(message: ElementRef<'_>) -> Result<String> {
    let owner = message.select(&OWNER_NAME).next().context("Telegram preview message owner name is missing")?;
    Ok(owner.text().map(str::trim).collect())
}

fn extract_text_html(message: ElementRef<'_>) -> String {
    let Some(text) = message.select(&MESSAGE_TEXT).next() else {
        return String::new();
    };
    let mut rendered = String::new();
    render_children(text, false, &mut rendered);
    rendered.trim().to_owned()
}

/// Render child nodes into `out`, in the subset of HTML accepted by the Telegram Bot API.
fn render_children(element: ElementRef<'_>, in_pre: bool, out: &mut String) {
    for child in element.children() {
        if let Some(child) = ElementRef::wrap(child) {
            render_element(child, in_pre, out);
        } else if let Node::Text(text) = child.value() {
            // Collapse markup indentation between tags so it does not become visible text, except inside <pre>.
            if !in_pre && !text.is_empty() && text.chars().all(char::is_whitespace) {
                out.push(if text.contains('\n') { '\n' } else { ' ' });
            } else {
                out.push_str(&escape_text(text));
            }
        }
    }
}

fn render_element(element: ElementRef<'_>, in_pre: bool, out: &mut String) {
    let name = element.value().name();
    if name == "br" {
        out.push('\n');
        return;
    }
    // Telegram renders emoji as styled <i> tags whose text content is the real emoji.
    if name == "i" && has_class(element, "emoji") {
        out.push_str(&escape_text(&element.text().collect::<String>()));
        return;
    }

    let href = element.value().attr("href");
    let tag = match name {
        "a" if href.is_some() => "a",
        "b" | "strong" => "b",
        "i" | "em" => "i",
        "u" => "u",
        "s" | "strike" | "del" => "s",
        "code" => "code",
        "pre" => "pre",
        // Other tags, and links without a target, keep only their content.
        _ => return render_children(element, in_pre, out),
    };
    out.push('<');
    out.push_str(tag);
    if let Some(href) = href.filter(|_| tag == "a") {
        out.push_str(" href=\"");
        out.push_str(&escape_attribute(href));
        out.push('"');
    }
    out.push('>');
    render_children(element, in_pre || tag == "pre", out);
    out.push_str("</");
    out.push_str(tag);
    out.push('>');
}

/// Return the media URLs of a post and whether it has media that the preview does not expose.
fn extract_media_urls(message: ElementRef<'_>) -> (Vec<String>, bool) {
    let mut media_urls = Vec::new();
    let mut media_unavailable = false;
    for media in message.select(&MEDIA_WRAP) {
        let media_url = if has_class(media, "tgme_widget_message_photo_wrap") {
            media
                .value()
                .attr("style")
                .and_then(|style| BACKGROUND_IMAGE.captures(style))
                .map(|found| found["url"].to_owned())
        } else {
            media.select(&VIDEO).next().and_then(|video| video.value().attr("src")).map(str::to_owned)
        };
        match media_url {
            Some(media_url) => media_urls.push(normalize_media_url(media_url)),
            None => media_unavailable = true,
        }
    }
    if media_urls.is_empty() && message.select(&MEDIA_NOT_SUPPORTED).next().is_some() {
        media_unavailable = true;
    }
    (media_urls, media_unavailable)
}

fn has_class(element: ElementRef<'_>, class_name: &str) -> bool {
    element.value().classes().any(|class| class == class_name)
}

/// Give protocol-relative preview media URLs the HTTPS scheme.
fn normalize_media_url(media_url: String) -> String {
    match media_url.strip_prefix("//") {
        Some(rest) => format!("https://{rest}"),
        None => media_url,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Preview HTML with posts of the channel, a post of another channel, and Telegram's markup quirks.
    const PREVIEW_HTML: &str = r#"
    <section>
      <div class="tgme_widget_message" data-post="Example/12">
        <a class="tgme_widget_message_owner_name" href="https://t.me/example"><span>Example News</span></a>
        <a class="tgme_widget_message_photo_wrap" style="background-image:url('//cdn.example/photo.jpg')"></a>
        <a class="tgme_widget_message_photo_wrap" style="background-image:url('https://cdn.example/photo-2.jpg')"></a>
        <div class="tgme_widget_message_video_wrap">
          <video class="tgme_widget_message_video" src="//cdn.example/video.mp4"></video>
        </div>
        <div class="tgme_widget_message_text js-message_text">
          Hello <b>bold</b> &amp; <code>a&lt;b</code><br>
          <a href="https://example.com/?a=1&amp;b=2">link</a>
          <i class="emoji"><b>👉</b></i>
        </div>
      </div>
      <div class="tgme_widget_message" data-post="other/99"></div>
      <div class="tgme_widget_message" data-post="example/14">
        <a class="tgme_widget_message_owner_name" href="https://t.me/example"><span>Example News</span></a>
        <div class="tgme_widget_message_text js-message_text">Second <pre><b>a</b>
  <b>b</b></pre></div>
      </div>
      <div class="tgme_widget_message" data-post="example/16">
        <a class="tgme_widget_message_owner_name" href="https://t.me/example"><span>Example News</span></a>
        <div class="message_media_not_supported_wrap">Please open Telegram to view this post</div>
      </div>
    </section>
    "#;

    #[test]
    fn parse_extracts_the_channel_posts_in_telegram_html() {
        let posts = parse_channel_preview("example", PREVIEW_HTML, 0).unwrap();

        assert_eq!(posts.iter().map(|post| post.message_id).collect::<Vec<_>>(), [12, 14, 16]);
        assert_eq!(posts[0].source_channel, "example");
        assert_eq!(posts[0].channel_display_name, "Example News");
        assert_eq!(posts[0].post_url, "https://t.me/example/12");
        assert_eq!(
            posts[0].media_urls,
            ["https://cdn.example/photo.jpg", "https://cdn.example/photo-2.jpg", "https://cdn.example/video.mp4"]
        );
        assert!(!posts[0].media_unavailable);
        assert_eq!(
            posts[0].text_html,
            "Hello <b>bold</b> &amp; <code>a&lt;b</code>\n\n<a href=\"https://example.com/?a=1&amp;b=2\">link</a>\n👉"
        );
        assert_eq!(posts[1].text_html, "Second <pre><b>a</b>\n  <b>b</b></pre>");
        assert_eq!(posts[2].text_html, "");
        assert!(posts[2].media_urls.is_empty());
        assert!(posts[2].media_unavailable);
    }

    #[test]
    fn parse_skips_committed_posts_before_reading_their_metadata() {
        let html = format!(r#"<div data-post="example/13"></div>{PREVIEW_HTML}"#);

        let posts = parse_channel_preview("example", &html, 14).unwrap();

        assert_eq!(posts.iter().map(|post| post.message_id).collect::<Vec<_>>(), [16]);
    }

    #[test]
    fn parse_rejects_a_new_post_without_an_owner_name() {
        assert!(parse_channel_preview("example", r#"<div data-post="example/1"></div>"#, 0).is_err());
    }
}
