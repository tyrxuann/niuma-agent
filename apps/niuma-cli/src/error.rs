//! Error types for the niuma-cli application.

use std::io;

use thiserror::Error;

/// CLI application errors.
#[derive(Debug, Error)]
pub enum CliError {
    /// Terminal I/O error.
    #[error("Terminal error: {0}")]
    Terminal(#[from] io::Error),

    /// TUI initialization error.
    #[error("Failed to initialize TUI: {0}")]
    TuiInit(String),

    /// Event handling error.
    #[error("Event handling error: {0}")]
    #[expect(dead_code, reason = "Public API for future event handling errors")]
    Event(String),
}

/// Result type for CLI operations.
pub type CliResult<T> = Result<T, CliError>;
