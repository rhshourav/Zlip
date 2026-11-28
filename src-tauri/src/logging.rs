//! Logging. Never log clipboard content: only ids, categories and sizes.

use std::path::Path;
use tracing_appender::{non_blocking::WorkerGuard, rolling};
use tracing_subscriber::{EnvFilter, fmt};

/// Daily-rolling log file under the data directory. Keep the guard alive.
pub fn init(dir: &Path) -> WorkerGuard {
    let _ = std::fs::create_dir_all(dir);
    let (writer, guard) = tracing_appender::non_blocking(rolling::daily(dir, "zlip.log"));
    let filter = EnvFilter::try_from_env("ZLIP_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .try_init();
    // A panic on a background thread is otherwise invisible (there is no console in
    // release builds). Put it in the log, with the thread name and location.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        tracing::error!(thread = thread.name().unwrap_or("unnamed"), "panic: {info}");
        previous(info);
    }));
    guard
}
