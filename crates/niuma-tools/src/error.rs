//! Error types for niuma-tools.

use thiserror::Error;

/// The main error type for niuma-tools.
#[derive(Debug, Error)]
pub enum Error {
    /// A generic error with a message.
    #[error("{0}")]
    Generic(String),
}

/// A specialized `Result` type for niuma-tools.
pub type Result<T> = std::result::Result<T, Error>;
