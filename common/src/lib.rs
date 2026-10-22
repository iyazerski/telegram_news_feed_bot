//! Pieces shared by the Rust services: settings loading, logging and shutdown, the database pool, and the Telegram
//! Bot API.

pub mod config;
pub mod database;
pub mod runtime;
pub mod telegram;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
