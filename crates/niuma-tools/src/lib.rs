//! Tool definitions and implementations for niuma agent.
//!
//! This crate provides tool abstractions and built-in tools that can be
//! used by the agent to interact with external systems.

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
