//! AI news recap microservice: researches the configured topics with a Rig agent signed in to ChatGPT and sends the
//! recap to the bot chat on schedule.
//!
//! `recapper` runs the service. `recapper login` signs in to ChatGPT with a device code.

mod config;
mod domain;
mod entrypoints;
mod infrastructure;
mod use_cases;

use anyhow::{Result, bail};

use crate::config::Configs;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let configs = Configs::load()?;
    common::runtime::configure_logging(&configs.log_level);
    match std::env::args().nth(1).as_deref() {
        None => entrypoints::runtime::run_forever(configs).await,
        Some("login") => infrastructure::ai::researcher::sign_in(&configs.recap_auth_dir).await,
        Some(command) => bail!("unknown command {command:?}; use no command to run the service, or `login`"),
    }
}
