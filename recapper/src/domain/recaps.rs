use std::collections::HashSet;

use url::Url;

// Tracking parameters differ between shares of one article, so they must not defeat deduplication.
const TRACKING_QUERY_PREFIXES: [&str; 5] = ["utm_", "fbclid", "gclid", "mc_", "ref"];

/// One researched news story.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecapStory {
    pub emoji: String,
    pub title: String,
    pub summary: String,
    pub source_name: String,
    pub url: String,
}

/// Return a comparable article URL without fragments, tracking parameters, or a trailing slash.
pub fn normalize_story_url(raw: &str) -> String {
    let Ok(url) = Url::parse(raw.trim()) else {
        return raw.trim().to_lowercase();
    };
    // Url::parse already lowercases the scheme and host.
    let mut normalized =
        format!("{}://{}", url.scheme(), &url[url::Position::BeforeUsername..url::Position::AfterPort]);
    normalized.push_str(url.path().trim_end_matches('/'));
    let query: Vec<(String, String)> = url
        .query_pairs()
        // Blank values carry no meaning, as with Python's parse_qsl defaults.
        .filter(|(key, value)| !value.is_empty() && !is_tracking_key(key))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    if !query.is_empty() {
        let encoded = url::form_urlencoded::Serializer::new(String::new()).extend_pairs(query).finish();
        normalized.push('?');
        normalized.push_str(&encoded);
    }
    normalized
}

fn is_tracking_key(key: &str) -> bool {
    let key = key.to_lowercase();
    TRACKING_QUERY_PREFIXES.iter().any(|prefix| key.starts_with(prefix))
}

/// Keep up to the limit of stories whose articles were neither sent before nor repeated in this recap.
pub fn select_new_stories(stories: Vec<RecapStory>, sent_urls: &[String], limit: usize) -> Vec<RecapStory> {
    let mut seen: HashSet<String> = sent_urls.iter().map(|url| normalize_story_url(url)).collect();
    stories.into_iter().filter(|story| seen.insert(normalize_story_url(&story.url))).take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_story(url: &str) -> RecapStory {
        RecapStory {
            emoji: "📰".to_owned(),
            title: "Title".to_owned(),
            summary: "Summary.".to_owned(),
            source_name: "Source".to_owned(),
            url: url.to_owned(),
        }
    }

    #[test]
    fn select_new_stories_skips_sent_and_repeated_articles_up_to_limit() {
        // Tracking, fragment, case, and trailing-slash variants are the same article.
        let stories = vec![
            build_story("https://News.example/a/?utm_source=x#top"),
            build_story("https://news.example/b"),
            build_story("https://news.example/b/?fbclid=1"),
            build_story("https://news.example/c?id=7"),
            build_story("https://news.example/d"),
        ];

        let selected = select_new_stories(stories, &["https://news.example/a".to_owned()], 2);

        let urls: Vec<&str> = selected.iter().map(|story| story.url.as_str()).collect();
        assert_eq!(urls, ["https://news.example/b", "https://news.example/c?id=7"]);
    }
}
