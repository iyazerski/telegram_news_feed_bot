/// Escape text content for Telegram HTML.
pub fn escape_text(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Escape a double-quoted Telegram HTML attribute value.
pub fn escape_attribute(value: &str) -> String {
    escape_text(value).replace('"', "&quot;").replace('\'', "&#x27;")
}
