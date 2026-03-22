//! Core types and utilities for niuma agent.
//!
//! This crate provides the foundational types, error handling, and utilities
//! used across all niuma crates.

#![warn(missing_docs)]
#![warn(rust_2024_compatibility)]
#![warn(missing_debug_implementations)]

pub mod config;
pub mod error;
pub mod session;

pub use config::StorageConfig;
pub use error::{Error, Result};
pub use session::{
    Backoff, ClarifyResult, ClarifyState, Confidence, DialogueState, ExecutionEvent, ExecutionPlan,
    ExecutionResult, ExecutionStrategy, FailureAction, MissingInfo, Session, Step, StepResult,
    Task, TaskBuilder, ToolResult, UserIntent,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::Generic("test error".to_string());
        assert_eq!(err.to_string(), "test error");
    }
}
