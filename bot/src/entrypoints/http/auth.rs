use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Why Mini App init data does not prove who opened the Mini App.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InitDataError {
    #[error("Telegram Mini App init data is missing")]
    Missing,
    #[error("Telegram Mini App init data is missing its signature")]
    MissingHash,
    #[error("Telegram Mini App init data is missing its auth date")]
    MissingAuthDate,
    #[error("Telegram Mini App init data is missing its user")]
    MissingUser,
    #[error("Telegram Mini App init data signature is invalid")]
    InvalidSignature,
    #[error("Telegram Mini App init data auth date is invalid")]
    InvalidAuthDate,
    #[error("Telegram Mini App init data is expired")]
    Expired,
    #[error("Telegram Mini App init data user is invalid")]
    InvalidUser,
}

#[derive(Deserialize)]
struct InitDataUser {
    id: i64,
}

/// Return the key that Telegram signs a bot's Mini App init data with, which depends only on the bot token.
pub fn init_data_secret_key(bot_token: &str) -> [u8; 32] {
    let mut mac = hmac(b"WebAppData");
    mac.update(bot_token.as_bytes());
    mac.finalize().into_bytes().into()
}

/// Return the ID of the Telegram user in signed Mini App init data that is at most `max_age_seconds` old.
pub fn validate_init_data(
    init_data: &str,
    secret_key: &[u8; 32],
    max_age_seconds: i64,
    now: DateTime<Utc>,
) -> Result<i64, InitDataError> {
    // A later duplicate field wins, and the sorted map gives the order that Telegram signs.
    let mut fields: BTreeMap<String, String> = url::form_urlencoded::parse(init_data.as_bytes()).into_owned().collect();
    let received_hash = fields.remove("hash").ok_or(InitDataError::MissingHash)?;
    let auth_date = fields.get("auth_date").ok_or(InitDataError::MissingAuthDate)?;
    let user = fields.get("user").ok_or(InitDataError::MissingUser)?;

    // Telegram signs the sorted `key=value` lines, joined by newlines.
    let mut mac = hmac(secret_key);
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 {
            mac.update(b"\n");
        }
        mac.update(key.as_bytes());
        mac.update(b"=");
        mac.update(value.as_bytes());
    }
    let received_hash = hex::decode(received_hash).map_err(|_| InitDataError::InvalidSignature)?;
    // verify_slice compares in constant time, so the answer time tells nothing about the right signature.
    mac.verify_slice(&received_hash).map_err(|_| InitDataError::InvalidSignature)?;

    let auth_date: i64 = auth_date.parse().map_err(|_| InitDataError::InvalidAuthDate)?;
    if auth_date + max_age_seconds < now.timestamp() {
        return Err(InitDataError::Expired);
    }
    let user: InitDataUser = serde_json::from_str(user).map_err(|_| InitDataError::InvalidUser)?;
    Ok(user.id)
}

fn hmac(key: &[u8]) -> HmacSha256 {
    HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any length")
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub const BOT_TOKEN: &str = "123:test-token";

    /// Return init data that Telegram would send for `user_id`, signed at `auth_date` with `bot_token`.
    pub fn signed_init_data(bot_token: &str, user_id: i64, auth_date: i64) -> String {
        let user = format!(r#"{{"id": {user_id}, "first_name": "Test"}}"#);
        let fields = [("auth_date", auth_date.to_string()), ("query_id", "AAE".to_owned()), ("user", user)];
        let data_check_string: Vec<String> = fields.iter().map(|(key, value)| format!("{key}={value}")).collect();
        let mut mac = hmac(&init_data_secret_key(bot_token));
        mac.update(data_check_string.join("\n").as_bytes());
        let hash = hex::encode(mac.finalize().into_bytes());
        url::form_urlencoded::Serializer::new(String::new()).extend_pairs(fields).append_pair("hash", &hash).finish()
    }

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap()
    }

    fn validate(init_data: &str) -> Result<i64, InitDataError> {
        validate_init_data(init_data, &init_data_secret_key(BOT_TOKEN), 3_600, now())
    }

    #[test]
    fn signed_init_data_names_its_user_until_it_expires() {
        let fresh = signed_init_data(BOT_TOKEN, 42, now().timestamp() - 60);
        let old = signed_init_data(BOT_TOKEN, 42, now().timestamp() - 3_601);

        assert_eq!(validate(&fresh), Ok(42));
        assert_eq!(validate(&old), Err(InitDataError::Expired));
    }

    #[test]
    fn init_data_with_a_wrong_or_missing_signature_is_rejected() {
        let other_bot = signed_init_data("456:other-token", 42, now().timestamp());
        let tampered = signed_init_data(BOT_TOKEN, 42, now().timestamp()).replace("42", "43");
        let unsigned = format!("auth_date={}&user=%7B%22id%22%3A42%7D", now().timestamp());

        for (init_data, error) in [
            (other_bot.as_str(), InitDataError::InvalidSignature),
            (tampered.as_str(), InitDataError::InvalidSignature),
            (unsigned.as_str(), InitDataError::MissingHash),
            ("hash=00&user=%7B%7D", InitDataError::MissingAuthDate),
        ] {
            assert_eq!(validate(init_data), Err(error), "{init_data}");
        }
    }
}
