//! Error types for niuma-llm.

use thiserror::Error;

/// The main error type for niuma-llm.
#[derive(Debug, Error)]
pub enum Error {
    /// A generic error with a message.
    #[error("{0}")]
    Generic(String),

    /// Streaming is not supported by this provider.
    #[error("Streaming is not supported by this provider")]
    StreamingNotSupported,

    /// The provider is not configured.
    #[error("Provider '{0}' is not configured")]
    ProviderNotConfigured(String),

    /// An environment variable was not found.
    #[error("Environment variable '{0}' not found")]
    EnvVarNotFound(String),

    /// Invalid environment variable syntax.
    #[error("Invalid environment variable syntax: {0}")]
    InvalidEnvVarSyntax(String),

    /// HTTP request failed.
    #[error("HTTP request failed: {0}")]
    HttpError(String),

    /// API returned an error response.
    #[error("API error: {message}")]
    ApiError {
        /// The error message.
        message: String,
        /// The error type (if provided by the API).
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Failed to parse API response.
    #[error("Failed to parse API response: {0}")]
    ParseError(String),

    /// Authentication failed.
    #[error("Authentication failed: {0}")]
    AuthenticationError(String),

    /// Rate limit exceeded.
    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    /// Request timed out.
    #[error("Request timed out")]
    Timeout,

    /// Invalid request.
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Model not found.
    #[error("Model '{0}' not found")]
    ModelNotFound(String),

    /// Content was filtered.
    #[error("Content was filtered: {0}")]
    ContentFiltered(String),
}

impl Error {
    /// Creates a new generic error.
    #[must_use]
    pub fn generic(msg: impl Into<String>) -> Self {
        Self::Generic(msg.into())
    }

    /// Creates a new HTTP error.
    #[must_use]
    pub fn http(msg: impl Into<String>) -> Self {
        Self::HttpError(msg.into())
    }

    /// Creates a new parse error.
    #[must_use]
    pub fn parse(msg: impl Into<String>) -> Self {
        Self::ParseError(msg.into())
    }

    /// Creates a new API error.
    #[must_use]
    pub fn api(msg: impl Into<String>) -> Self {
        Self::ApiError {
            message: msg.into(),
            source: None,
        }
    }

    /// Creates a new API error with a source.
    #[must_use]
    pub fn api_with_source(
        msg: impl Into<String>,
        source: Box<dyn std::error::Error + Send + Sync>,
    ) -> Self {
        Self::ApiError {
            message: msg.into(),
            source: Some(source),
        }
    }
}

/// A specialized `Result` type for niuma-llm.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::generic("test error");
        assert_eq!(err.to_string(), "test error");

        let err = Error::ProviderNotConfigured("openai".to_string());
        assert_eq!(err.to_string(), "Provider 'openai' is not configured");
    }

    #[test]
    fn test_error_helpers() {
        let err = Error::http("connection refused");
        assert!(matches!(err, Error::HttpError(_)));

        let err = Error::parse("invalid JSON");
        assert!(matches!(err, Error::ParseError(_)));
    }
}
