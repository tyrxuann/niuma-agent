//! Tool registry for managing built-in and MCP tools.
//!
//! This module provides [`ToolRegistry`] which manages all available tools
//! including built-in tools and MCP server tools.

use std::{collections::HashMap, sync::Arc};

use serde_json::Value;
use tracing::{debug, info, warn};

use crate::{
    Error, Result, Tool,
    builtin::{FileReadTool, FileWriteTool, HttpTool, ShellTool},
    mcp::MCPServer,
};

/// A registry that manages all available tools.
///
/// The registry holds both built-in tools and MCP server tools,
/// providing a unified interface for tool discovery and execution.
///
/// # Example
///
/// ```rust,ignore
/// use niuma_tools::registry::ToolRegistry;
///
/// let registry = ToolRegistry::new();
///
/// // Get a tool by name
/// let tool = registry.get("file_read");
///
/// // List all available tools
/// for name in registry.list_tools() {
///     println!("Tool: {}", name);
/// }
/// ```
#[derive(Debug)]
pub struct ToolRegistry {
    /// Built-in tools indexed by name.
    builtins: HashMap<String, Arc<dyn Tool>>,
    /// MCP server tools indexed by name.
    mcp_servers: HashMap<String, MCPServer>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    /// Creates a new empty tool registry.
    ///
    /// Use [`register_builtin`](Self::register_builtin) to add built-in tools,
    /// or [`register_mcp`](Self::register_mcp) to add MCP server tools.
    #[must_use]
    pub fn new() -> Self {
        Self {
            builtins: HashMap::new(),
            mcp_servers: HashMap::new(),
        }
    }

    /// Creates a new tool registry with all built-in tools registered.
    ///
    /// This registers the following built-in tools:
    /// - `file_read`: Read file contents
    /// - `file_write`: Write file contents
    /// - `shell`: Execute shell commands
    /// - `http`: Make HTTP requests
    #[must_use]
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        registry.register_builtin("file_read", Arc::new(FileReadTool::new()));
        registry.register_builtin("file_write", Arc::new(FileWriteTool::new()));
        registry.register_builtin("shell", Arc::new(ShellTool::new()));
        registry.register_builtin("http", Arc::new(HttpTool::new()));
        info!("Registered 4 built-in tools");
        registry
    }

    /// Registers a built-in tool.
    ///
    /// # Arguments
    ///
    /// * `name` - The name to register the tool under
    /// * `tool` - The tool implementation wrapped in an `Arc`
    ///
    /// # Panics
    ///
    /// Does not panic. If a tool with the same name exists, it will be replaced
    /// and a warning will be logged.
    pub fn register_builtin(&mut self, name: &str, tool: Arc<dyn Tool>) {
        if self.builtins.contains_key(name) {
            warn!("Replacing existing builtin tool: {}", name);
        }
        debug!("Registering builtin tool: {}", name);
        self.builtins.insert(name.to_string(), tool);
    }

    /// Registers an MCP server.
    ///
    /// # Arguments
    ///
    /// * `name` - The name to register the MCP server under
    /// * `server` - The MCP server configuration
    ///
    /// # Panics
    ///
    /// Does not panic. If an MCP server with the same name exists, it will be replaced
    /// and a warning will be logged.
    pub fn register_mcp(&mut self, name: &str, server: MCPServer) {
        if self.mcp_servers.contains_key(name) {
            warn!("Replacing existing MCP server: {}", name);
        }
        debug!("Registering MCP server: {}", name);
        self.mcp_servers.insert(name.to_string(), server);
    }

    /// Gets a tool by name.
    ///
    /// First checks built-in tools, then MCP server tools.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the tool to retrieve
    ///
    /// # Returns
    ///
    /// Returns `Some(Arc<dyn Tool>)` if found, `None` otherwise.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.builtins.get(name).cloned()
    }

    /// Gets an MCP server by name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the MCP server to retrieve
    ///
    /// # Returns
    ///
    /// Returns `Some(&MCPServer)` if found, `None` otherwise.
    #[must_use]
    pub fn get_mcp(&self, name: &str) -> Option<&MCPServer> {
        self.mcp_servers.get(name)
    }

    /// Lists all registered tool names.
    ///
    /// # Returns
    ///
    /// A vector of all registered tool names (both built-in and MCP).
    #[must_use]
    pub fn list_tools(&self) -> Vec<String> {
        self.builtins.keys().cloned().collect()
    }

    /// Lists all registered MCP server names.
    ///
    /// # Returns
    ///
    /// A vector of all registered MCP server names.
    #[must_use]
    pub fn list_mcp_servers(&self) -> Vec<String> {
        self.mcp_servers.keys().cloned().collect()
    }

    /// Executes a tool by name with the given arguments.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the tool to execute
    /// * `args` - The JSON arguments to pass to the tool
    ///
    /// # Returns
    ///
    /// The result of the tool execution.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The tool is not found
    /// - The tool execution fails
    pub async fn execute(&self, name: &str, args: Value) -> Result<Value> {
        let tool = self.get(name).ok_or_else(|| Error::ToolNotFound {
            name: name.to_string(),
        })?;
        debug!("Executing tool: {} with args: {:?}", name, args);
        tool.execute(args).await
    }

    /// Returns the number of registered tools (built-in only).
    #[must_use]
    pub fn tool_count(&self) -> usize {
        self.builtins.len()
    }

    /// Returns the number of registered MCP servers.
    #[must_use]
    pub fn mcp_count(&self) -> usize {
        self.mcp_servers.len()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::ToolRegistry;

    #[test]
    fn test_registry_new() {
        let registry = ToolRegistry::new();
        assert_eq!(registry.tool_count(), 0);
        assert_eq!(registry.mcp_count(), 0);
    }

    #[test]
    fn test_registry_default() {
        let registry = ToolRegistry::default();
        assert_eq!(registry.tool_count(), 0);
    }

    #[test]
    fn test_registry_with_builtins() {
        let registry = ToolRegistry::with_builtins();
        assert_eq!(registry.tool_count(), 4);

        // Check all built-in tools are registered
        let tools = registry.list_tools();
        assert!(tools.contains(&"file_read".to_string()));
        assert!(tools.contains(&"file_write".to_string()));
        assert!(tools.contains(&"shell".to_string()));
        assert!(tools.contains(&"http".to_string()));
    }

    #[test]
    fn test_registry_get() {
        let registry = ToolRegistry::with_builtins();
        assert!(registry.get("file_read").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[tokio::test]
    async fn test_registry_execute_not_found() {
        let registry = ToolRegistry::new();
        let result = registry.execute("nonexistent", json!({})).await;
        assert!(result.is_err());
    }
}
