//! Agent orchestration and execution logic for niuma.
//!
//! This crate provides the main agent implementation that coordinates
//! LLM interactions and tool execution.

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
