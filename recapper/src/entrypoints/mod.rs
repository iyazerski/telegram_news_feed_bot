pub mod runtime;

use std::io::IsTerminal;

use tracing_subscriber::EnvFilter;

/// Log at the configured level, given in Python's names such as `INFO`, with colors only in a terminal.
pub fn configure_logging(log_level: &str) {
    let filter = EnvFilter::try_new(log_level.to_lowercase()).unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_ansi(std::io::stdout().is_terminal())
        .init();
}
