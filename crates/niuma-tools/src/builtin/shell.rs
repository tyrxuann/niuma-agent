//! Shell command execution tool implementation.

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{process::Command, time::timeout};
use tracing::{debug, warn};

use crate::{Error, Result, Tool};

/// Default timeout for command execution (60 seconds).
const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// Maximum timeout allowed (10 minutes).
const MAX_TIMEOUT_SECS: u64 = 600;

/// Maximum output size (1 MB).
const MAX_OUTPUT_SIZE: usize = 1024 * 1024;

/// Input parameters for the `shell` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ShellInput {
    /// The command to execute.
    command: String,
    /// Command arguments.
    #[serde(default)]
    args: Vec<String>,
    /// Working directory for the command.
    #[serde(default)]
    cwd: Option<String>,
    /// Timeout in seconds.
    #[serde(default = "default_timeout")]
    timeout_secs: u64,
    /// Environment variables to set.
    #[serde(default)]
    env: std::collections::HashMap<String, String>,
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_SECS
}

/// Output from a shell command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ShellOutput {
    /// Whether the command succeeded (exit code 0).
    success: bool,
    /// The exit code of the command.
    exit_code: Option<i32>,
    /// Standard output from the command.
    stdout: String,
    /// Standard error from the command.
    stderr: String,
    /// Duration of execution in milliseconds.
    duration_ms: u64,
}

/// A tool for executing shell commands.
///
/// This tool allows running shell commands with proper timeout handling,
/// output capture, and security considerations.
///
/// # Security
///
/// - Commands have a configurable timeout (default 60s, max 10 minutes)
/// - Output is truncated if too large
/// - Environment variables can be set but are isolated to the command
///
/// # Warning
///
/// This tool executes arbitrary commands. Use with caution and ensure
/// proper input validation in higher-level code.
#[derive(Debug, Clone)]
pub struct ShellTool {
    /// Default working directory.
    default_cwd: Option<std::path::PathBuf>,
}

impl Default for ShellTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellTool {
    /// Creates a new shell tool.
    #[must_use]
    pub fn new() -> Self {
        Self { default_cwd: None }
    }

    /// Creates a new shell tool with a default working directory.
    #[must_use]
    pub fn with_default_cwd(cwd: std::path::PathBuf) -> Self {
        Self {
            default_cwd: Some(cwd),
        }
    }

    /// Truncates output if it exceeds the maximum size.
    fn truncate_output(output: String) -> String {
        if output.len() > MAX_OUTPUT_SIZE {
            let truncated_len = MAX_OUTPUT_SIZE;
            let mut truncated = output.chars().take(truncated_len).collect::<String>();
            truncated.push_str("\n... [output truncated]");
            truncated
        } else {
            output
        }
    }

    /// Validates the timeout value.
    fn validate_timeout(&self, timeout_secs: u64) -> Result<Duration> {
        if timeout_secs == 0 {
            return Err(Error::InvalidArguments {
                tool: self.name().to_string(),
                message: "timeout must be greater than 0".to_string(),
            });
        }
        if timeout_secs > MAX_TIMEOUT_SECS {
            warn!(
                "Requested timeout {}s exceeds maximum, capping at {}s",
                timeout_secs, MAX_TIMEOUT_SECS
            );
        }
        let capped = std::cmp::min(timeout_secs, MAX_TIMEOUT_SECS);
        Ok(Duration::from_secs(capped))
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Executes a shell command and returns its output. Supports timeout, working directory, and \
         environment variables. Use with caution as commands run with the same permissions as the \
         agent."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command to execute (e.g., 'ls', 'git', 'npm')"
                },
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Arguments to pass to the command"
                },
                "cwd": {
                    "type": "string",
                    "description": "Working directory for the command (default: current directory)"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 60, max: 600)",
                    "minimum": 1,
                    "maximum": MAX_TIMEOUT_SECS
                },
                "env": {
                    "type": "object",
                    "additionalProperties": { "type": "string" },
                    "description": "Environment variables to set for the command"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let input: ShellInput =
            serde_json::from_value(args).map_err(|e| Error::InvalidArguments {
                tool: self.name().to_string(),
                message: e.to_string(),
            })?;

        debug!("Executing command: {} {:?}", input.command, input.args);

        let timeout_duration = self.validate_timeout(input.timeout_secs)?;

        let start = std::time::Instant::now();

        // Build the command
        let mut cmd = Command::new(&input.command);
        cmd.args(&input.args);

        // Set working directory
        if let Some(ref cwd) = input.cwd {
            cmd.current_dir(cwd);
        } else if let Some(ref default_cwd) = self.default_cwd {
            cmd.current_dir(default_cwd);
        }

        // Set environment variables
        for (key, value) in &input.env {
            cmd.env(key, value);
        }

        // Execute with timeout and capture stdout/stderr
        let result = timeout(timeout_duration, cmd.output()).await;

        let output = match result {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                return Err(Error::ShellCommand {
                    message: format!("failed to execute command '{}': {e}", input.command),
                });
            }
            Err(_) => {
                return Err(Error::ShellCommand {
                    message: format!(
                        "command '{}' timed out after {} seconds",
                        input.command, input.timeout_secs
                    ),
                });
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        // Decode output
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let shell_output = ShellOutput {
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout: Self::truncate_output(stdout),
            stderr: Self::truncate_output(stderr),
            duration_ms,
        };

        debug!(
            "Command completed: {} (exit code: {:?}, duration: {}ms)",
            input.command, shell_output.exit_code, shell_output.duration_ms
        );

        if !shell_output.success {
            debug!("Command stderr: {}", shell_output.stderr);
        }

        Ok(json!(shell_output))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::Tool;

    #[test]
    fn test_tool_name() {
        let tool = ShellTool::new();
        assert_eq!(tool.name(), "shell");
    }

    #[test]
    fn test_tool_description() {
        let tool = ShellTool::new();
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_input_schema() {
        let tool = ShellTool::new();
        let schema = tool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["command"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("command")));
    }

    #[test]
    fn test_truncate_output_normal() {
        let output = "Hello, World!".to_string();
        let truncated = ShellTool::truncate_output(output);
        assert_eq!(truncated, "Hello, World!");
    }

    #[test]
    fn test_truncate_output_too_large() {
        let output = "x".repeat(MAX_OUTPUT_SIZE + 1000);
        let truncated = ShellTool::truncate_output(output);
        assert!(truncated.len() < MAX_OUTPUT_SIZE + 100);
        assert!(truncated.ends_with("... [output truncated]"));
    }

    #[test]
    fn test_validate_timeout_zero() {
        let tool = ShellTool::new();
        let result = tool.validate_timeout(0);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_timeout_too_large() {
        let tool = ShellTool::new();
        let result = tool.validate_timeout(MAX_TIMEOUT_SECS + 100);
        assert!(result.is_ok());
        let duration = result.expect("should succeed");
        assert_eq!(duration, Duration::from_secs(MAX_TIMEOUT_SECS));
    }

    #[tokio::test]
    async fn test_execute_echo() {
        let tool = ShellTool::new();
        let args = json!({
            "command": "echo",
            "args": ["Hello, World!"]
        });
        let result = tool.execute(args).await;

        assert!(result.is_ok());
        let output = result.expect("execute should succeed");
        assert!(output["success"].as_bool().unwrap());
        assert!(output["stdout"].as_str().unwrap().contains("Hello, World!"));
    }

    #[tokio::test]
    async fn test_execute_with_env() {
        let tool = ShellTool::new();

        #[cfg(unix)]
        let args = json!({
            "command": "sh",
            "args": ["-c", "echo $MY_VAR"],
            "env": { "MY_VAR": "test_value" }
        });

        #[cfg(windows)]
        let args = json!({
            "command": "cmd",
            "args": ["/C", "echo %MY_VAR%"],
            "env": { "MY_VAR": "test_value" }
        });

        let result = tool.execute(args).await;

        assert!(result.is_ok());
        let output = result.expect("execute should succeed");
        assert!(output["stdout"].as_str().unwrap().contains("test_value"));
    }

    #[tokio::test]
    async fn test_execute_nonexistent_command() {
        let tool = ShellTool::new();
        let args = json!({
            "command": "nonexistent_command_that_does_not_exist_xyz123"
        });
        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_timeout() {
        let tool = ShellTool::new();

        #[cfg(unix)]
        let args = json!({
            "command": "sleep",
            "args": ["10"],
            "timeout_secs": 1
        });

        #[cfg(windows)]
        let args = json!({
            "command": "timeout",
            "args": ["/T", "10"],
            "timeout_secs": 1
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
        let err = result.expect_err("should timeout");
        assert!(err.to_string().contains("timed out"));
    }
}
