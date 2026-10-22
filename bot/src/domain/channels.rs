use url::Url;

/// Why a channel username or URL names no public channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ChannelNameError {
    #[error("Only t.me or telegram.me channel URLs are supported")]
    UnsupportedHost,
    #[error("Telegram channel URL must include a username")]
    UrlWithoutUsername,
    #[error("Telegram channel URL must not include a message path")]
    UrlWithMessagePath,
    #[error("Channel username is required")]
    Empty,
    #[error("Channel username must not include a message path")]
    WithMessagePath,
}

/// Return the lowercase username of a public channel given as `name`, `@name`, or a `t.me/name` URL.
pub fn normalize_channel_username(username_or_url: &str) -> Result<String, ChannelNameError> {
    let value = username_or_url.trim();
    let username = if value.starts_with("http://") || value.starts_with("https://") {
        let url = Url::parse(value).map_err(|_| ChannelNameError::UnsupportedHost)?;
        if !matches!(url.host_str(), Some("t.me" | "telegram.me")) {
            return Err(ChannelNameError::UnsupportedHost);
        }
        let mut segments = url.path_segments().into_iter().flatten().filter(|segment| !segment.is_empty());
        let username = segments.next().ok_or(ChannelNameError::UrlWithoutUsername)?;
        if segments.next().is_some() {
            return Err(ChannelNameError::UrlWithMessagePath);
        }
        username.to_owned()
    } else {
        value.to_owned()
    };

    let username = username.strip_prefix('@').unwrap_or(&username).trim().to_lowercase();
    if username.is_empty() {
        return Err(ChannelNameError::Empty);
    }
    if username.contains('/') {
        return Err(ChannelNameError::WithMessagePath);
    }
    Ok(username)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames_and_channel_urls_become_lowercase_usernames() {
        for input in [
            "Example",
            " @Example ",
            "https://t.me/Example",
            "https://telegram.me/example/",
            "https://t.me/example?x=1",
        ] {
            assert_eq!(normalize_channel_username(input), Ok("example".to_owned()), "{input}");
        }
    }

    #[test]
    fn inputs_without_exactly_one_channel_are_rejected() {
        let cases = [
            ("https://example.com/example", ChannelNameError::UnsupportedHost),
            ("https://t.me", ChannelNameError::UrlWithoutUsername),
            ("https://t.me/", ChannelNameError::UrlWithoutUsername),
            ("https://t.me/example/42", ChannelNameError::UrlWithMessagePath),
            (" @ ", ChannelNameError::Empty),
            ("t.me/example", ChannelNameError::WithMessagePath),
        ];
        for (input, error) in cases {
            assert_eq!(normalize_channel_username(input), Err(error), "{input}");
        }
    }
}
