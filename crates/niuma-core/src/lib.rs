//! Core types and utilities for niuma agent.
//!
//! This crate provides the foundational types, error handling, and utilities
//! used across all niuma crates.

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
