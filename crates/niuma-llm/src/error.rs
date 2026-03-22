//! Error types for niuma-llm.

use thiserror::Error;

/// The main error type for niuma-llm.
#[derive(Debug, Error)]
pub enum Error {
    /// A generic error with a message.
    #[error("{0}")]
    Generic(String),
}

/// A specialized `Result` type for niuma-llm.
pub type Result<T> = std::result::Result<T, Error>;
