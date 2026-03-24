//! Agent orchestration and execution logic for niuma.
//!
//! This crate provides the main agent implementation that coordinates
//! LLM interactions and tool execution.
//!
//! # Core Components
//!
//! - [`IntentParser`]: Classifies user input into intents and execution strategies
//! - [`Clarifier`]: Socrates-style dialogue for gathering missing information
//! - [`Executor`]: Runs execution plans with confidence checks
//!
//! # Example
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use niuma_agent::{Agent, IntentParser, Clarifier, Executor};
//! use niuma_llm::{ClaudeProvider, LLMProvider};
//! use niuma_tools::ToolRegistry;
//!
//! // Create components
//! let llm = Arc::new(ClaudeProvider::new("your-api-key"));
//! let tools = Arc::new(ToolRegistry::with_builtins());
//!
//! let intent_parser = IntentParser::new(Arc::clone(&llm));
//! let clarifier = Clarifier::new(Arc::clone(&llm));
//! let executor = Executor::new(Arc::clone(&llm), Arc::clone(&tools));
//! ```

#![warn(missing_docs)]
#![warn(rust_2024_compatibility)]
#![warn(missing_debug_implementations)]

pub mod clarifier;
pub mod error;
pub mod executor;
pub mod intent;
pub mod persistence;
pub mod plan_cache;
pub mod scheduler;

pub use clarifier::{Clarifier, ClarifyContext};
pub use error::{Error, Result};
pub use executor::Executor;
pub use intent::{IntentClassification, IntentParser};
pub use plan_cache::PlanCache;
pub use scheduler::{NoopExecutor, StorageConfig, TaskExecutor, TaskScheduler};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::Generic("test error".to_string());
        assert_eq!(err.to_string(), "test error");
    }

    #[test]
    fn test_error_helpers() {
        let err = Error::intent_parse("bad format");
        assert!(err.to_string().contains("bad format"));

        let err = Error::tool_not_found("my_tool");
        assert!(err.to_string().contains("my_tool"));

        let err = Error::session_not_found("abc123");
        assert!(err.to_string().contains("abc123"));
    }
}
