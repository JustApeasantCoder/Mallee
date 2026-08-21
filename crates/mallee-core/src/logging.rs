use std::path::Path;

use anyhow::Result;
use dee_bugee_rust::{LoggerConfig, LoggerGuard, non_blocking_layer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub struct LoggingGuard {
    _guard: LoggerGuard,
}

pub fn init_logging(data_dir: &Path, source: &str) -> Result<LoggingGuard> {
    let path = data_dir.join("logs").join("Mallee.jsonl");
    let (layer, guard) = non_blocking_layer(LoggerConfig::new(path, source))?;
    tracing_subscriber::registry().with(layer).try_init()?;
    tracing::info!(
        subsystem = "lifecycle",
        event = "application.started",
        status = "ready",
        "[Mallee] Application started"
    );
    Ok(LoggingGuard { _guard: guard })
}
