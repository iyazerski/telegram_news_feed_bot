//! Feed microservice: polls the public web previews of subscribed Telegram channels and reposts their new posts to
//! the bot chat, one channel cursor at a time.

mod config;
mod domain;
mod entrypoints;
mod infrastructure;
mod use_cases;

use anyhow::Result;

use crate::config::Configs;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let configs = Configs::load()?;
    common::runtime::configure_logging(&configs.log_level);
    entrypoints::runtime::run_forever(configs).await
}
