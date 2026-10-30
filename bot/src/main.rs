//! Bot microservice: answers Telegram commands, receives updates by webhook or long polling, and serves the Mini App
//! that manages channel subscriptions, and the poll interval.

mod config;
mod domain;
mod entrypoints;
mod infrastructure;
#[cfg(test)]
mod testing;
mod use_cases;

use anyhow::Result;

use crate::config::Configs;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let configs = Configs::load()?;
    common::runtime::configure_logging(&configs.log_level);
    entrypoints::runtime::run_forever(configs).await
}
