use common::telegram::html::{escape_attribute, escape_text};

use crate::domain::recaps::RecapStory;

/// Telegram's text message limit.
const TEXT_MESSAGE_LIMIT: usize = 4096;
pub const EMPTY_RECAP_TEXT: &str = "No important new stories for your topics since the last recap.";

/// Build one Telegram HTML recap message, dropping trailing stories that exceed the length limit.
pub fn build_recap_message(stories: &[RecapStory], topics: &[String]) -> String {
    let header = format!("🗞 <b>News recap</b>\n<i>{}</i>", escape_text(&topics.join(" · ")));
    if stories.is_empty() {
        return format!("{header}\n\n{EMPTY_RECAP_TEXT}");
    }

    // Telegram counts visible text, which is never longer than the HTML, so this bound is safe.
    let mut message = header;
    let mut length = message.chars().count();
    for (index, block) in stories.iter().map(build_story).enumerate() {
        length += 2 + block.chars().count();
        // The first story always goes out, even when it alone is too long.
        if index > 0 && length > TEXT_MESSAGE_LIMIT {
            break;
        }
        message.push_str("\n\n");
        message.push_str(&block);
    }
    message
}

/// Build one escaped recap story block with its source link.
fn build_story(story: &RecapStory) -> String {
    format!(
        "{} <b>{}</b>\n{}\n🔗 <a href=\"{}\">{}</a>",
        escape_text(&story.emoji),
        escape_text(&story.title),
        escape_text(&story.summary),
        escape_attribute(&story.url),
        escape_text(&story.source_name),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_message_escapes_story_text_and_links_sources() {
        let story = RecapStory {
            emoji: "🤖".to_owned(),
            title: "A <b> & B".to_owned(),
            summary: "Why it matters > hype.".to_owned(),
            source_name: "News & Co".to_owned(),
            url: "https://news.example/a?x=1&y=\"2\"".to_owned(),
        };

        let message = build_recap_message(&[story], &["AI".to_owned(), "R&D".to_owned()]);

        assert_eq!(
            message,
            "🗞 <b>News recap</b>\n<i>AI · R&amp;D</i>\n\n\
             🤖 <b>A &lt;b&gt; &amp; B</b>\n\
             Why it matters &gt; hype.\n\
             🔗 <a href=\"https://news.example/a?x=1&amp;y=&quot;2&quot;\">News &amp; Co</a>"
        );
    }

    #[test]
    fn build_message_reports_empty_recaps_and_drops_overflowing_stories() {
        assert!(build_recap_message(&[], &["AI".to_owned()]).ends_with(EMPTY_RECAP_TEXT));

        let stories: Vec<RecapStory> = (0..5)
            .map(|index| RecapStory {
                emoji: "📰".to_owned(),
                title: format!("Story {index}"),
                summary: "x".repeat(1000),
                source_name: "S".to_owned(),
                url: "https://s.example".to_owned(),
            })
            .collect();
        let message = build_recap_message(&stories, &["AI".to_owned()]);

        assert!(message.chars().count() <= TEXT_MESSAGE_LIMIT);
        assert!(message.contains("Story 2"));
        assert!(!message.contains("Story 3"));
    }
}
