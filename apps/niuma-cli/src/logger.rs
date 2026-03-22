//! Logging initialization for niuma-cli.
//!
//! This module provides file-based logging with rotation support.

use tracing_subscriber::{EnvFilter, util::SubscriberInitExt};

use crate::config::Config;

/// Initializes the logging system with file output.
///
/// This function sets up tracing to write logs to a file with rotation.
/// The log level and file path are determined by the configuration.
///
/// # Errors
///
/// Returns an error if the log directory cannot be created or the
/// tracing subscriber cannot be initialized.
pub fn init_logging(config: &Config) -> Result<(), LogError> {
    let log_path = config.log_file_path();
    let log_dir = log_path
        .parent()
        .ok_or_else(|| LogError::InvalidPath(log_path.display().to_string()))?;

    // Create log directory if it doesn't exist
    if !log_dir.exists() {
        // Use blocking create since this is before async runtime
        #[allow(clippy::disallowed_methods)]
        std::fs::create_dir_all(log_dir)
            .map_err(|e| LogError::CreateDir(log_dir.display().to_string(), e))?;
    }

    // Create a file appender with rotation
    let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix(
            log_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("niuma"),
        )
        .filename_suffix("log")
        .max_log_files(config.logging.max_files)
        .build(log_dir)
        .map_err(|e| LogError::CreateAppender(log_dir.display().to_string(), e.to_string()))?;

    // Parse log level from config
    let log_level = parse_log_level(config.log_level());

    // Create env filter with the configured level
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level));

    // Initialize the subscriber with file output only
    let subscriber = tracing_subscriber::fmt()
        .with_writer(file_appender)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(true)
        .with_env_filter(env_filter)
        .finish();

    subscriber
        .try_init()
        .map_err(|e| LogError::Init(e.to_string()))?;

    tracing::info!(
        log_file = %log_path.display(),
        level = %config.log_level(),
        "Logging initialized"
    );

    Ok(())
}

/// Parses a log level string into a tracing Level.
fn parse_log_level(level: &str) -> String {
    match level.to_lowercase().as_str() {
        "trace" => "trace",
        "debug" => "debug",
        "info" => "info",
        "warn" | "warning" => "warn",
        "error" => "error",
        _ => "info",
    }
    .to_string()
}

/// Logging error types.
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    /// Invalid log file path.
    #[error("Invalid log file path: {0}")]
    InvalidPath(String),

    /// Failed to create log directory.
    #[error("Failed to create log directory '{0}': {1}")]
    CreateDir(String, std::io::Error),

    /// Failed to create file appender.
    #[error("Failed to create log file appender in '{0}': {1}")]
    CreateAppender(String, String),

    /// Failed to initialize logging.
    #[error("Failed to initialize logging: {0}")]
    Init(String),
}
