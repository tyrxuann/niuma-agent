//! Error types for niuma-core.

use thiserror::Error;

/// The main error type for niuma-core.
#[derive(Debug, Error)]
pub enum Error {
    /// A generic error with a message.
    #[error("{0}")]
    Generic(String),
}

/// A specialized `Result` type for niuma-core.
pub type Result<T> = std::result::Result<T, Error>;
