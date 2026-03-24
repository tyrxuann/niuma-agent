//! Tool definitions and implementations for niuma agent.
//!
//! This crate provides tool abstractions and built-in tools that can be
//! used by the agent to interact with external systems.
//!
//! # Overview
//!
//! The crate is organized around the following components:
//!
//! - [`Tool`] trait: The core abstraction for all tools
//! - [`ToolRegistry`]: Manages and provides access to all registered tools
//! - Built-in tools: [`FileReadTool`], [`FileWriteTool`], [`ShellTool`], [`HttpTool`]
//! - MCP support: [`MCPServer`] for connecting to external MCP servers
//!
//! # Example
//!
//! ```rust,ignore
//! use niuma_tools::{ToolRegistry, Tool};
//! use serde_json::json;
//!
//! // Create a registry with all built-in tools
//! let registry = ToolRegistry::with_builtins();
//!
//! // Execute a tool
//! let result = registry.execute("file_read", json!({
//!     "path": "/path/to/file.txt"
//! })).await;
//!
//! // List available tools
//! for tool_name in registry.list_tools() {
//!     println!("Available tool: {}", tool_name);
//! }
//! ```

#![warn(missing_docs)]
#![warn(rust_2024_compatibility)]
#![warn(missing_debug_implementations)]

pub mod builtin;
pub mod error;
pub mod mcp;
pub mod registry;
pub mod tool;

pub use error::{Error, Result};
pub use mcp::{MCPServer, MCPServerBuilder, MCPServerConfig, MCPToolInfo};
pub use registry::ToolRegistry;
pub use tool::Tool;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::Generic("test error".to_string());
        assert_eq!(err.to_string(), "test error");
    }

    #[test]
    fn test_registry_with_builtins() {
        let registry = ToolRegistry::with_builtins();
        assert!(registry.tool_count() >= 4);
    }
}
