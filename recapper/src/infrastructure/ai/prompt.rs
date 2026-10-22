use chrono::{DateTime, Utc};
use common::settings::RecapSettings;
use serde::Deserialize;

use crate::domain::recaps::RecapStory;

#[derive(Deserialize)]
struct RecapStoryOutput {
    emoji: String,
    title: String,
    summary: String,
    source_name: String,
    url: String,
}

#[derive(Deserialize)]
struct RecapOutput {
    stories: Vec<RecapStoryOutput>,
}

/// Build the instructions that make the agent research and return one recap as JSON.
pub fn build_news_recap_prompt(
    settings: &RecapSettings,
    now: DateTime<Utc>,
    since: DateTime<Utc>,
    story_count: usize,
    sent_stories: &[(String, String)],
) -> String {
    let topics: Vec<String> = settings.topics.iter().map(|topic| format!("- {topic}")).collect();
    let sent: Vec<String> = sent_stories.iter().map(|(title, url)| format!("- {title} ({url})")).collect();
    let sent = if sent.is_empty() { "- none".to_owned() } else { sent.join("\n") };
    let topics = topics.join("\n");
    let language = &settings.language;
    let now = now.format("%Y-%m-%d %H:%M");
    let since = since.format("%Y-%m-%d %H:%M");
    // Models assume their training cutoff is today, so the prompt states the real date and window.
    format!(
        r#"You prepare a short news digest for a Telegram chat.

Current date and time: {now} UTC. Trust this date over your own assumptions.
Time window: news published or substantially updated since {since} UTC.

Topics:
{topics}

Steps:
1. Use web_search with several focused queries per topic. Include the current month and year in queries.
2. Pick the 3 to {story_count} most important stories in the time window across all topics. Rank them by impact and by how widely reputable outlets cover them. Cover several topics when each has important news, but never add a weak story only to fill a topic.
3. For every story, link one specific article from a reputable original source. Never link homepages, search pages, tag pages, or aggregators. Only use URLs that appeared in your search or fetch results. Use fetch_content when you need to confirm a date or detail.
4. Skip stories that repeat these already sent ones, even when another outlet reports them:
{sent}

Write title and summary in {language}. Keep source_name as the outlet's own name.
Each summary has 2 short sentences: what happened, and why it matters.
Pick one emoji that fits each story.

Reply with only this JSON object, without Markdown fences or any other text:
{{"stories": [{{"emoji": "...", "title": "...", "summary": "...", "source_name": "...", "url": "https://..."}}]}}
Return {{"stories": []}} when nothing important and new happened in the time window."#
    )
}

/// Parse the agent's recap reply into stories, tolerating text around the JSON object.
pub fn parse_news_recap_output(output: &str) -> Result<Vec<RecapStory>, String> {
    let (Some(start), Some(end)) = (output.find('{'), output.rfind('}')) else {
        return Err("the reply does not contain a JSON object".to_owned());
    };
    if end < start {
        return Err("the reply does not contain a JSON object".to_owned());
    }
    let recap: RecapOutput = serde_json::from_str(&output[start..=end])
        .map_err(|error| format!("the reply is not a valid recap: {error}"))?;
    recap
        .stories
        .into_iter()
        .map(|story| {
            Ok(RecapStory {
                emoji: bounded("emoji", &story.emoji, 16)?,
                title: bounded("title", &story.title, 300)?,
                summary: bounded("summary", &story.summary, 1000)?,
                source_name: bounded("source_name", &story.source_name, 120)?,
                url: http_url(&story.url)?,
            })
        })
        .collect()
}

/// Return the trimmed value when it has between 1 and `max_chars` characters.
fn bounded(field: &str, value: &str, max_chars: usize) -> Result<String, String> {
    let value = value.trim();
    let length = value.chars().count();
    if length == 0 || length > max_chars {
        return Err(format!("the reply is not a valid recap: {field} must have 1 to {max_chars} characters"));
    }
    Ok(value.to_owned())
}

/// Return the URL when it is a whitespace-free http(s) link, so the Telegram link cannot run script.
fn http_url(value: &str) -> Result<String, String> {
    let has_scheme = value.starts_with("https://") || value.starts_with("http://");
    let rest = value.split_once("://").map_or("", |(_, rest)| rest);
    if !has_scheme || rest.is_empty() || value.chars().any(char::is_whitespace) || value.len() > 2000 {
        return Err(format!("the reply is not a valid recap: {value} is not an http(s) URL"));
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn prompt_states_date_window_language_and_sent_stories() {
        let settings = RecapSettings {
            topics: vec!["AI".to_owned(), "Space".to_owned()],
            language: "Russian".to_owned(),
            interval_seconds: 21_600,
            enabled: true,
        };

        let prompt = build_news_recap_prompt(
            &settings,
            Utc.with_ymd_and_hms(2026, 10, 20, 18, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 10, 19, 18, 0, 0).unwrap(),
            5,
            &[("Old story".to_owned(), "https://news.example/old".to_owned())],
        );

        assert!(prompt.contains("Current date and time: 2026-10-20 18:00 UTC"));
        assert!(prompt.contains("since 2026-10-19 18:00 UTC"));
        assert!(prompt.contains("- AI\n- Space"));
        assert!(prompt.contains("in Russian"));
        assert!(prompt.contains("- Old story (https://news.example/old)"));
        assert!(prompt.contains(r#"{"stories": []}"#));
    }

    #[test]
    fn parse_output_tolerates_fences_and_rejects_invalid_links() {
        let output = r#"```json
        {"stories": [{"emoji": "🚀", "title": " Launch ", "summary": "It flew.", "source_name": "Reuters",
        "url": "https://reuters.example/launch"}]}
        ```"#;

        assert_eq!(
            parse_news_recap_output(output).unwrap(),
            [RecapStory {
                emoji: "🚀".to_owned(),
                title: "Launch".to_owned(),
                summary: "It flew.".to_owned(),
                source_name: "Reuters".to_owned(),
                url: "https://reuters.example/launch".to_owned(),
            }]
        );
        let invalid = parse_news_recap_output(&output.replace("https://reuters.example/launch", "javascript:alert(1)"));
        assert!(invalid.unwrap_err().contains("not a valid recap"));
        assert!(parse_news_recap_output("No news today.").unwrap_err().contains("does not contain a JSON object"));
    }
}
