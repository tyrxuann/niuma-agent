//! MCP (Model Context Protocol) client implementation.
//!
//! This module provides the MCP client for connecting to external MCP servers
//! and registering their tools with the tool registry.

use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::{Error, Result};

/// Configuration for an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPServerConfig {
    /// The command to start the MCP server.
    pub command: String,
    /// Arguments to pass to the command.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables to set.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Working directory for the server process.
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

/// Represents a connected MCP server.
///
/// This struct holds the configuration for an MCP server and provides
/// methods for starting the server and communicating with it.
#[derive(Debug, Clone)]
pub struct MCPServer {
    /// The name of the MCP server.
    name: String,
    /// The server configuration.
    config: MCPServerConfig,
    /// Whether the server is currently connected.
    connected: bool,
}

impl MCPServer {
    /// Creates a new MCP server instance.
    ///
    /// # Arguments
    ///
    /// * `name` - A unique name for this server
    /// * `config` - The server configuration
    #[must_use]
    pub fn new(name: String, config: MCPServerConfig) -> Self {
        Self {
            name,
            config,
            connected: false,
        }
    }

    /// Returns the name of this server.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the server configuration.
    #[must_use]
    pub fn config(&self) -> &MCPServerConfig {
        &self.config
    }

    /// Checks if the server is connected.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Starts the MCP server process.
    ///
    /// This method spawns the server process and establishes
    /// communication channels.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The server process cannot be started
    /// - The server fails to initialize
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting MCP server: {}", self.name);
        debug!("Server config: {:?}", self.config);

        // TODO: Implement actual process spawning and stdio communication
        // This is a skeleton implementation
        //
        // The full implementation would:
        // 1. Spawn the process with the configured command and args
        // 2. Set up stdin/stdout pipes for JSON-RPC communication
        // 3. Send the initialize request
        // 4. Receive the initialize response with capabilities
        // 5. Store the process handle for later use

        self.connected = true;
        Ok(())
    }

    /// Stops the MCP server process.
    ///
    /// # Errors
    ///
    /// Returns an error if the server cannot be stopped gracefully.
    pub async fn stop(&mut self) -> Result<()> {
        if !self.connected {
            return Ok(());
        }

        info!("Stopping MCP server: {}", self.name);

        // TODO: Implement actual process termination
        // This would:
        // 1. Send a shutdown request to the server
        // 2. Wait for the process to exit
        // 3. Force kill if it doesn't exit gracefully

        self.connected = false;
        Ok(())
    }

    /// Lists the tools available on this MCP server.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The server is not connected
    /// - The tools/list request fails
    pub async fn list_tools(&self) -> Result<Vec<MCPToolInfo>> {
        if !self.connected {
            return Err(Error::MCPServer {
                server: self.name.clone(),
                message: "server is not connected".to_string(),
            });
        }

        debug!("Listing tools for MCP server: {}", self.name);

        // TODO: Implement actual tools/list request
        // This would send a JSON-RPC request to the server
        // and parse the response into MCPToolInfo structs

        Ok(Vec::new())
    }

    /// Executes a tool on this MCP server.
    ///
    /// # Arguments
    ///
    /// * `tool_name` - The name of the tool to execute
    /// * `args` - The arguments to pass to the tool
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The server is not connected
    /// - The tool doesn't exist
    /// - The tool execution fails
    pub async fn execute_tool(
        &self,
        tool_name: &str,
        _args: serde_json::Value,
    ) -> Result<serde_json::Value> {
        if !self.connected {
            return Err(Error::MCPServer {
                server: self.name.clone(),
                message: "server is not connected".to_string(),
            });
        }

        debug!(
            "Executing tool '{}' on MCP server '{}'",
            tool_name, self.name
        );

        // TODO: Implement actual tools/call request
        // This would send a JSON-RPC request to the server
        // with the tool name and arguments

        Err(Error::MCPServer {
            server: self.name.clone(),
            message: format!("tool execution not implemented for '{tool_name}'"),
        })
    }
}

/// Information about a tool provided by an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPToolInfo {
    /// The name of the tool.
    pub name: String,
    /// A description of what the tool does.
    pub description: String,
    /// The JSON schema for the tool's input parameters.
    pub input_schema: serde_json::Value,
}

/// Builder for creating MCP server configurations.
#[derive(Debug, Default)]
pub struct MCPServerBuilder {
    command: Option<String>,
    args: Vec<String>,
    env: HashMap<String, String>,
    cwd: Option<PathBuf>,
}

impl MCPServerBuilder {
    /// Creates a new builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the command to run.
    #[must_use]
    pub fn command(mut self, command: impl Into<String>) -> Self {
        self.command = Some(command.into());
        self
    }

    /// Adds an argument to the command.
    #[must_use]
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Adds multiple arguments to the command.
    #[must_use]
    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Sets an environment variable.
    #[must_use]
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Sets the working directory.
    #[must_use]
    pub fn cwd(mut self, cwd: PathBuf) -> Self {
        self.cwd = Some(cwd);
        self
    }

    /// Builds the MCP server configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the command is not set.
    pub fn build(self) -> Result<MCPServerConfig> {
        let command = self.command.ok_or_else(|| Error::MCPServer {
            server: "builder".to_string(),
            message: "command is required".to_string(),
        })?;

        Ok(MCPServerConfig {
            command,
            args: self.args,
            env: self.env,
            cwd: self.cwd,
        })
    }
}

impl From<MCPServerConfig> for MCPServerBuilder {
    fn from(config: MCPServerConfig) -> Self {
        Self {
            command: Some(config.command),
            args: config.args,
            env: config.env,
            cwd: config.cwd,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_server_config_creation() {
        let config = MCPServerConfig {
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "@playwright/mcp".to_string()],
            env: HashMap::new(),
            cwd: None,
        };

        assert_eq!(config.command, "npx");
        assert_eq!(config.args.len(), 2);
    }

    #[test]
    fn test_mcp_server_creation() {
        let config = MCPServerConfig {
            command: "echo".to_string(),
            args: Vec::new(),
            env: HashMap::new(),
            cwd: None,
        };

        let server = MCPServer::new("test".to_string(), config);
        assert_eq!(server.name(), "test");
        assert!(!server.is_connected());
    }

    #[test]
    fn test_mcp_server_builder() {
        let config = MCPServerBuilder::new()
            .command("npx")
            .arg("-y")
            .arg("@playwright/mcp")
            .env("HEADLESS", "true")
            .build();

        assert!(config.is_ok());
        let config = config.expect("config should be valid");
        assert_eq!(config.command, "npx");
        assert_eq!(config.args, vec!["-y", "@playwright/mcp"]);
        assert_eq!(config.env.get("HEADLESS"), Some(&"true".to_string()));
    }

    #[test]
    fn test_mcp_server_builder_missing_command() {
        let result = MCPServerBuilder::new().build();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mcp_server_execute_not_connected() {
        let config = MCPServerConfig {
            command: "echo".to_string(),
            args: Vec::new(),
            env: HashMap::new(),
            cwd: None,
        };

        let server = MCPServer::new("test".to_string(), config);
        let result = server
            .execute_tool("test_tool", serde_json::json!({}))
            .await;
        assert!(result.is_err());
    }
}
