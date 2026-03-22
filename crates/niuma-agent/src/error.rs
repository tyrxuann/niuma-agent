//! Error types for niuma-agent.

use thiserror::Error;

/// The main error type for niuma-agent.
#[derive(Debug, Error)]
pub enum Error {
    /// A generic error with a message.
    #[error("{0}")]
    Generic(String),
}

/// A specialized `Result` type for niuma-agent.
pub type Result<T> = std::result::Result<T, Error>;
