//! Tool trait definition for niuma agent.
//!
//! This module provides the core [`Tool`] trait that all tools must implement,
//! whether they are built-in tools or MCP server tools.

use async_trait::async_trait;
use serde_json::Value;

use crate::Result;

/// A trait representing a tool that can be executed by the agent.
///
/// All tools must implement this trait to be registered in the [`ToolRegistry`].
/// Tools are async, thread-safe, and can be executed with JSON arguments.
///
/// # Example
///
/// ```rust,ignore
/// use niuma_tools::{Tool, Result};
/// use async_trait::async_trait;
/// use serde_json::{Value, json};
///
/// struct EchoTool;
///
/// #[async_trait]
/// impl Tool for EchoTool {
///     fn name(&self) -> &str {
///         "echo"
///     }
///
///     fn description(&self) -> &str {
///         "Echoes back the input message"
///     }
///
///     fn input_schema(&self) -> Value {
///         json!({
///             "type": "object",
///             "properties": {
///                 "message": {
///                     "type": "string",
///                     "description": "The message to echo back"
///                 }
///             },
///             "required": ["message"]
///         })
///     }
///
///     async fn execute(&self, args: Value) -> Result<Value> {
///         let message = args["message"].as_str().unwrap_or("");
///         Ok(json!({ "echoed": message }))
///     }
/// }
/// ```
///
/// [`ToolRegistry`]: crate::registry::ToolRegistry
#[async_trait]
pub trait Tool: Send + Sync + std::fmt::Debug {
    /// Returns the unique name of this tool.
    ///
    /// Tool names should be lowercase with underscores (e.g., `file_read`, `http_get`).
    fn name(&self) -> &str;

    /// Returns a human-readable description of what this tool does.
    ///
    /// This description is used by the LLM to understand when to use this tool.
    fn description(&self) -> &str;

    /// Returns the JSON Schema for the tool's input parameters.
    ///
    /// The schema should describe the expected structure of the `args` parameter
    /// passed to [`execute`](Self::execute).
    fn input_schema(&self) -> Value;

    /// Executes the tool with the given arguments.
    ///
    /// # Arguments
    ///
    /// * `args` - A JSON value containing the tool's input parameters. The structure should match
    ///   the schema returned by [`input_schema`](Self::input_schema).
    ///
    /// # Returns
    ///
    /// A `Result` containing the tool's output as a JSON value, or an error if execution failed.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The arguments don't match the expected schema
    /// - The tool execution fails (e.g., file not found, command failed)
    /// - Any other runtime error occurs
    async fn execute(&self, args: Value) -> Result<Value>;
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use serde_json::{Value, json};

    use super::Tool;
    use crate::Result;

    /// A simple echo tool for testing the trait interface.
    #[derive(Debug)]
    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }

        fn description(&self) -> &str {
            "Echoes back the input message"
        }

        fn input_schema(&self) -> Value {
            json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "The message to echo back"
                    }
                },
                "required": ["message"]
            })
        }

        async fn execute(&self, args: Value) -> Result<Value> {
            let message = args["message"].as_str().unwrap_or("");
            Ok(json!({ "echoed": message }))
        }
    }

    #[test]
    fn test_tool_name() {
        let tool = EchoTool;
        assert_eq!(tool.name(), "echo");
    }

    #[test]
    fn test_tool_description() {
        let tool = EchoTool;
        assert_eq!(tool.description(), "Echoes back the input message");
    }

    #[test]
    fn test_tool_input_schema() {
        let tool = EchoTool;
        let schema = tool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["message"].is_object());
    }

    #[tokio::test]
    async fn test_tool_execute() {
        let tool = EchoTool;
        let args = json!({ "message": "hello" });
        let result = tool.execute(args).await;
        assert!(result.is_ok());
        let output = result.expect("execute should succeed");
        assert_eq!(output["echoed"], "hello");
    }
}
