use anyhow::Result;
use sqlx::postgres::{PgPool, PgPoolOptions};

/// Create a lazily connected pool, so a service starts while the database is down.
pub fn connect_lazy(database_url: &str, max_connections: u32) -> Result<PgPool> {
    Ok(PgPoolOptions::new().max_connections(max_connections).connect_lazy(database_url)?)
}
