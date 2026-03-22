//! LLM integration layer for niuma agent.
//!
//! This crate provides abstractions and implementations for interacting
//! with various LLM providers.

#![warn(missing_docs)]
#![warn(rust_2024_compatibility)]
#![warn(missing_debug_implementations)]

pub mod error;

pub use error::{Error, Result};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::Generic("test error".to_string());
        assert_eq!(err.to_string(), "test error");
    }
}
