//! Error types for niuma-agent.

use thiserror::Error;

/// The main error type for niuma-agent.
#[derive(Debug, Error)]
pub enum Error {
    /// A generic error with a message.
    #[error("{0}")]
    Generic(String),

    /// Failed to parse intent classification.
    #[error("Intent parse error: {0}")]
    IntentParse(String),

    /// Clarifier error.
    #[error("Clarifier error: {0}")]
    Clarifier(String),

    /// Executor error.
    #[error("Executor error: {0}")]
    Executor(String),

    /// Tool not found.
    #[error("Tool not found: {name}")]
    ToolNotFound {
        /// The name of the tool that was not found.
        name: String,
    },

    /// Session not found.
    #[error("Session not found: {id}")]
    SessionNotFound {
        /// The ID of the session that was not found.
        id: String,
    },

    /// Invalid execution plan.
    #[error("Invalid execution plan: {0}")]
    InvalidPlan(String),

    /// Plan execution failed.
    #[error("Plan execution failed: {0}")]
    ExecutionFailed(String),

    /// LLM provider error.
    #[error("LLM provider error: {0}")]
    LLMError(String),

    /// Task scheduler error.
    #[error("Task scheduler error: {0}")]
    Scheduler(String),
}

impl Error {
    /// Creates a new generic error.
    #[must_use]
    pub fn generic(msg: impl Into<String>) -> Self {
        Self::Generic(msg.into())
    }

    /// Creates a new intent parse error.
    #[must_use]
    pub fn intent_parse(msg: impl Into<String>) -> Self {
        Self::IntentParse(msg.into())
    }

    /// Creates a new clarifier error.
    #[must_use]
    pub fn clarifier(msg: impl Into<String>) -> Self {
        Self::Clarifier(msg.into())
    }

    /// Creates a new executor error.
    #[must_use]
    pub fn executor(msg: impl Into<String>) -> Self {
        Self::Executor(msg.into())
    }

    /// Creates a new invalid plan error.
    #[must_use]
    pub fn invalid_plan(msg: impl Into<String>) -> Self {
        Self::InvalidPlan(msg.into())
    }

    /// Creates a new execution failed error.
    #[must_use]
    pub fn execution_failed(msg: impl Into<String>) -> Self {
        Self::ExecutionFailed(msg.into())
    }

    /// Creates a new tool not found error.
    #[must_use]
    pub fn tool_not_found(name: impl Into<String>) -> Self {
        Self::ToolNotFound { name: name.into() }
    }

    /// Creates a new session not found error.
    #[must_use]
    pub fn session_not_found(id: impl Into<String>) -> Self {
        Self::SessionNotFound { id: id.into() }
    }

    /// Creates a new LLM error.
    #[must_use]
    pub fn llm(msg: impl Into<String>) -> Self {
        Self::LLMError(msg.into())
    }
}

impl From<niuma_llm::Error> for Error {
    fn from(e: niuma_llm::Error) -> Self {
        Self::LLMError(e.to_string())
    }
}

impl From<niuma_tools::Error> for Error {
    fn from(e: niuma_tools::Error) -> Self {
        Self::Executor(e.to_string())
    }
}

/// A specialized `Result` type for niuma-agent.
pub type Result<T> = std::result::Result<T, Error>;
