use anyhow::Result;
use sqlx::postgres::{PgPool, PgPoolOptions};

/// The `app_setting` key of the bot chat that receives posts and recaps, which the bot stores on /start.
pub const DESTINATION_CHAT_ID_KEY: &str = "destination_chat_id";

/// Create a lazily connected pool, so a service starts while the database is down.
pub fn connect_lazy(database_url: &str, max_connections: u32) -> Result<PgPool> {
    Ok(PgPoolOptions::new().max_connections(max_connections).connect_lazy(&postgres_url(database_url))?)
}

/// Return the database URL in the form sqlx accepts.
fn postgres_url(database_url: &str) -> String {
    // The shared URL names SQLAlchemy's driver, as in `postgresql+psycopg://`, which sqlx does not understand.
    match database_url.split_once("://") {
        Some((scheme, rest)) => {
            let scheme = scheme.split_once('+').map_or(scheme, |(base, _)| base);
            format!("{scheme}://{rest}")
        }
        None => database_url.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_url_drops_the_sqlalchemy_driver() {
        assert_eq!(postgres_url("postgresql+psycopg://user:pass@db:5432/news"), "postgresql://user:pass@db:5432/news");
        assert_eq!(postgres_url("postgresql://db/news"), "postgresql://db/news");
    }
}
