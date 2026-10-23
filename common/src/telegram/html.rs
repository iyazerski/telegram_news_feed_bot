/// Escape text content for Telegram HTML.
pub fn escape_text(value: &str) -> String {
    escape(value, false)
}

/// Escape a double-quoted Telegram HTML attribute value.
pub fn escape_attribute(value: &str) -> String {
    escape(value, true)
}

/// Escape markup characters in one pass, and quotes too in an attribute value.
fn escape(value: &str, attribute: bool) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' if attribute => escaped.push_str("&quot;"),
            '\'' if attribute => escaped.push_str("&#x27;"),
            _ => escaped.push(character),
        }
    }
    escaped
}
