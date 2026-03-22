//! Error types for niuma-tools.

use std::path::PathBuf;

use thiserror::Error;

/// The main error type for niuma-tools.
#[derive(Debug, Error)]
pub enum Error {
    /// A generic error with a message.
    #[error("{0}")]
    Generic(String),

    /// The requested tool was not found.
    #[error("Tool not found: {name}")]
    ToolNotFound {
        /// The name of the tool that was not found.
        name: String,
    },

    /// Invalid arguments provided to the tool.
    #[error("Invalid arguments for tool '{tool}': {message}")]
    InvalidArguments {
        /// The name of the tool.
        tool: String,
        /// A description of what was invalid.
        message: String,
    },

    /// File operation error.
    #[error("File operation failed for '{path}': {message}")]
    FileOperation {
        /// The path involved in the operation.
        path: PathBuf,
        /// A description of the error.
        message: String,
    },

    /// Path validation error (e.g., directory traversal attempt).
    #[error("Invalid path '{path}': {reason}")]
    InvalidPath {
        /// The path that was invalid.
        path: PathBuf,
        /// The reason why the path is invalid.
        reason: String,
    },

    /// Shell command execution error.
    #[error("Shell command failed: {message}")]
    ShellCommand {
        /// A description of the error.
        message: String,
    },

    /// HTTP request error.
    #[error("HTTP request failed: {message}")]
    HttpRequest {
        /// A description of the error.
        message: String,
    },

    /// MCP server error.
    #[error("MCP server error for '{server}': {message}")]
    MCPServer {
        /// The name of the MCP server.
        server: String,
        /// A description of the error.
        message: String,
    },

    /// JSON parsing or serialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// HTTP client error from reqwest.
    #[error("HTTP client error: {0}")]
    Reqwest(#[from] reqwest::Error),
}

/// A specialized `Result` type for niuma-tools.
pub type Result<T> = std::result::Result<T, Error>;
