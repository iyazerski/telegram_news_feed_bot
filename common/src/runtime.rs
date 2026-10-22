use std::io::IsTerminal;

use tracing::error;
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

/// Resolve on SIGTERM, which Kubernetes sends before it kills the pod, or on Ctrl-C locally.
pub async fn shutdown_signal() {
    let mut terminate = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        Ok(terminate) => terminate,
        Err(signal_error) => {
            error!("Could not listen for SIGTERM: {signal_error}");
            let _ = tokio::signal::ctrl_c().await;
            return;
        }
    };
    tokio::select! {
        _ = terminate.recv() => {}
        _ = tokio::signal::ctrl_c() => {}
    }
}
