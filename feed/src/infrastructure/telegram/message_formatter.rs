use common::telegram::html::escape_text;
use scraper::Html;

use crate::domain::posts::DiscoveredPost;

// Telegram's limits, which it counts in UTF-16 code units of visible text, so an emoji counts twice.
const MEDIA_CAPTION_LIMIT: usize = 1024;
const TEXT_MESSAGE_LIMIT: usize = 4096;

/// Build a media caption with the whole post, or only the channel header followed by text messages with the body
/// when the post does not fit.
pub fn build_media_messages(post: &DiscoveredPost) -> (String, Vec<String>) {
    let message = build_message_text(post);
    if visible_length(&message) <= MEDIA_CAPTION_LIMIT {
        return (message, Vec::new());
    }
    (build_channel_header(post), build_body_messages(post))
}

/// Build one or more Telegram HTML messages that together hold the whole post.
pub fn build_text_messages(post: &DiscoveredPost) -> Vec<String> {
    let message = build_message_text(post);
    if visible_length(&message) <= TEXT_MESSAGE_LIMIT {
        return vec![message];
    }

    // Split messages lose their formatting, as an HTML tag cut in two would make Telegram reject the message.
    // Channel names have at most 128 characters, so the header always leaves room in the first message.
    let header = build_channel_header(post);
    let body = html_to_plain_text(&build_message_body(post));
    let header_length = utf16_length(&post.channel_display_name);
    let (first_body, rest) = split_at_utf16(&body, TEXT_MESSAGE_LIMIT.saturating_sub(header_length + 2));
    let mut messages = vec![format!("{header}\n\n{}", escape_text(first_body))];
    messages.extend(split_plain_text(rest, TEXT_MESSAGE_LIMIT).into_iter().map(escape_text));
    messages
}

/// Build follow-up messages for a post whose caption held only the channel header.
fn build_body_messages(post: &DiscoveredPost) -> Vec<String> {
    let body = build_message_body(post);
    let body_text = html_to_plain_text(&body);
    if utf16_length(&body_text) <= TEXT_MESSAGE_LIMIT {
        return vec![body];
    }
    split_plain_text(&body_text, TEXT_MESSAGE_LIMIT).into_iter().map(escape_text).collect()
}

/// Build the whole repost: the channel header, then the body.
fn build_message_text(post: &DiscoveredPost) -> String {
    format!("{}\n\n{}", build_channel_header(post), build_message_body(post))
}

/// Build the post body, with the source link appended when media is unavailable.
fn build_message_body(post: &DiscoveredPost) -> String {
    if !post.text_html.is_empty() && !post.media_unavailable {
        return post.text_html.clone();
    }
    let source_link = escape_text(&post.post_url);
    if post.text_html.is_empty() { source_link } else { format!("{}\n\n{source_link}", post.text_html) }
}

/// Build a bold, underlined channel header that does not affect link previews.
fn build_channel_header(post: &DiscoveredPost) -> String {
    format!("<b><u>{}</u></b>", escape_text(&post.channel_display_name))
}

/// Return the visible text of Telegram HTML, as Telegram counts it against its limits.
pub fn html_to_plain_text(html: &str) -> String {
    Html::parse_fragment(html).root_element().text().collect::<String>().trim().to_owned()
}

fn visible_length(html: &str) -> usize {
    utf16_length(&html_to_plain_text(html))
}

fn utf16_length(text: &str) -> usize {
    text.encode_utf16().count()
}

/// Split text into chunks of at most `limit` UTF-16 code units.
fn split_plain_text(text: &str, limit: usize) -> Vec<&str> {
    let mut chunks = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let (chunk, tail) = split_at_utf16(rest, limit);
        chunks.push(chunk);
        rest = tail;
    }
    chunks
}

/// Split text after the most characters that fit in `limit` UTF-16 code units, never inside a character.
fn split_at_utf16(text: &str, limit: usize) -> (&str, &str) {
    let mut length = 0;
    let index = text
        .char_indices()
        .find(|(_, character)| {
            length += character.len_utf16();
            length > limit
        })
        .map_or(text.len(), |(index, _)| index);
    text.split_at(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post(text_html: &str) -> DiscoveredPost {
        DiscoveredPost {
            source_channel: "example".to_owned(),
            channel_display_name: "Example News".to_owned(),
            message_id: 42,
            text_html: text_html.to_owned(),
            media_urls: Vec::new(),
            post_url: "https://t.me/example/42".to_owned(),
            media_unavailable: false,
        }
    }

    #[test]
    fn oversized_text_is_split_without_losing_characters() {
        // Each emoji is two UTF-16 code units, so 2,500 of them exceed the limit that 2,500 characters would not.
        for body in ["я".repeat(5000), "😀".repeat(2500)] {
            let messages = build_text_messages(&post(&body));

            let plain: Vec<String> = messages.iter().map(|message| html_to_plain_text(message)).collect();
            assert_eq!(plain.len(), 2);
            assert_eq!(plain.concat(), format!("Example News\n\n{body}"));
            assert!(plain.iter().all(|message| utf16_length(message) <= TEXT_MESSAGE_LIMIT));
        }
    }

    #[test]
    fn plain_text_keeps_blank_lines_between_tags() {
        // Every newline Telegram renders counts, so limits are not undercounted.
        assert_eq!(html_to_plain_text("<i>a &amp; b</i>\n\n<a href=\"https://example.com\">c</a>"), "a & b\n\nc");
    }

    #[test]
    fn unavailable_media_appends_the_source_link() {
        let unavailable = DiscoveredPost { media_unavailable: true, ..post("Post text") };

        assert_eq!(
            build_text_messages(&unavailable),
            ["<b><u>Example News</u></b>\n\nPost text\n\nhttps://t.me/example/42"]
        );
        assert_eq!(
            build_text_messages(&DiscoveredPost { media_unavailable: true, ..post("") }),
            ["<b><u>Example News</u></b>\n\nhttps://t.me/example/42"]
        );
    }
}
