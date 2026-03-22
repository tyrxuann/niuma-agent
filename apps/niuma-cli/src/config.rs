//! Configuration management for niuma-cli.
//!
//! This module provides configuration loading and management, combining
//! settings from LLM, MCP servers, storage, and logging.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use niuma_core::StorageConfig;
use niuma_llm::config::LLMConfig;
use serde::{Deserialize, Serialize};

// ============================================================================
// Default values
// ============================================================================

/// Default config file name.
const DEFAULT_CONFIG_FILE: &str = "config.yaml";

/// Default log level.
const DEFAULT_LOG_LEVEL: &str = "info";

/// Default log file name.
const DEFAULT_LOG_FILE: &str = "niuma.log";

// ============================================================================
// Main configuration
// ============================================================================

/// Application configuration.
///
/// This is the top-level configuration structure that contains all
/// settings for the niuma agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// LLM provider configuration.
    #[serde(default)]
    pub llm: LLMConfig,

    /// MCP server configurations.
    #[serde(default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,

    /// Storage paths configuration.
    #[serde(default)]
    pub storage: StorageConfig,

    /// Logging configuration.
    #[serde(default)]
    pub logging: LoggingConfig,
}

impl Config {
    /// Returns the default config file path.
    ///
    /// The path is resolved in the following order:
    /// 1. `./config.yaml` (current directory)
    /// 2. `~/.config/niuma/config.yaml` (user config)
    /// 3. `/etc/niuma/config.yaml` (system config)
    #[must_use]
    pub fn default_path() -> PathBuf {
        // Check current directory first
        let local_config = PathBuf::from(DEFAULT_CONFIG_FILE);
        if local_config.exists() {
            return local_config;
        }

        // Check user config directory
        if let Some(home) = dirs::home_dir() {
            let user_config = home.join(".config").join("niuma").join(DEFAULT_CONFIG_FILE);
            if user_config.exists() {
                return user_config;
            }
        }

        // Fall back to current directory (even if it doesn't exist)
        PathBuf::from(DEFAULT_CONFIG_FILE)
    }

    /// Loads configuration from a YAML file.
    ///
    /// If the file doesn't exist, returns default configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be parsed.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Ok(Self::default());
        }

        // Use blocking read since config is loaded before async runtime
        #[allow(clippy::disallowed_methods)]
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::Read(path.display().to_string(), e))?;

        let mut config: Self = serde_yaml::from_str(&content)
            .map_err(|e| ConfigError::Parse(path.display().to_string(), e))?;

        // Expand environment variables
        config.expand_env();

        Ok(config)
    }

    /// Expands environment variables in all configuration sections.
    fn expand_env(&mut self) {
        // LLM config expands its own env vars
        let _ = self.llm.expand_env();

        // MCP servers
        for server in self.mcp_servers.values_mut() {
            server.expand_env();
        }

        // Storage
        self.storage.expand_env();

        // Logging
        self.logging.expand_env();
    }

    /// Returns the log file path.
    #[must_use]
    pub fn log_file_path(&self) -> PathBuf {
        let dir = if self.logging.logs_dir.is_empty() {
            self.storage.logs_dir.clone()
        } else {
            PathBuf::from(&self.logging.logs_dir)
        };

        if self.logging.log_file.is_empty() {
            dir.join(DEFAULT_LOG_FILE)
        } else {
            dir.join(&self.logging.log_file)
        }
    }

    /// Returns the log level as a tracing level string.
    #[must_use]
    pub fn log_level(&self) -> &str {
        if self.logging.level.is_empty() {
            DEFAULT_LOG_LEVEL
        } else {
            &self.logging.level
        }
    }
}

// ============================================================================
// MCP server configuration
// ============================================================================

/// MCP server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    /// The command to execute.
    pub command: String,

    /// Arguments to pass to the command.
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables to set.
    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl McpServerConfig {
    /// Expands environment variables in the command and environment values.
    fn expand_env(&mut self) {
        self.command = expand_env_var(&self.command);
        for value in self.env.values_mut() {
            *value = expand_env_var(value);
        }
    }
}

// ============================================================================
// Logging configuration
// ============================================================================

/// Logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingConfig {
    /// Log level: trace, debug, info, warn, error.
    #[serde(default)]
    pub level: String,

    /// Directory for log files (overrides storage.logs_dir if set).
    #[serde(default)]
    pub logs_dir: String,

    /// Log file name.
    #[serde(default)]
    pub log_file: String,

    /// Maximum log file size in MB before rotation.
    #[serde(default = "default_max_file_size")]
    pub max_file_size_mb: u64,

    /// Maximum number of rotated log files to keep.
    #[serde(default = "default_max_files")]
    pub max_files: usize,
}

fn default_max_file_size() -> u64 {
    10 // 10 MB
}

fn default_max_files() -> usize {
    5
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: String::new(),
            logs_dir: String::new(),
            log_file: String::new(),
            max_file_size_mb: default_max_file_size(),
            max_files: default_max_files(),
        }
    }
}

impl LoggingConfig {
    /// Expands environment variables in paths.
    fn expand_env(&mut self) {
        self.logs_dir = expand_env_var(&self.logs_dir);
        self.log_file = expand_env_var(&self.log_file);
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Expands environment variables in a string.
///
/// Supports `${VAR_NAME}` syntax. If the variable is not found,
/// the original string is kept.
fn expand_env_var(s: &str) -> String {
    let mut result = s.to_string();
    let mut start = 0;

    while let Some(begin) = result[start..].find("${") {
        let begin = start + begin;
        if let Some(end) = result[begin..].find('}') {
            let end = begin + end;
            let var_name = &result[begin + 2..end];

            if let Ok(value) = std::env::var(var_name) {
                result.replace_range(begin..=end, &value);
                start = begin;
            } else {
                start = end + 1;
            }
        } else {
            break;
        }
    }

    result
}

// ============================================================================
// Error types
// ============================================================================

/// Configuration error types.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Failed to read configuration file.
    #[error("Failed to read config file '{0}': {1}")]
    Read(String, std::io::Error),

    /// Failed to parse configuration file.
    #[error("Failed to parse config file '{0}': {1}")]
    Parse(String, serde_yaml::Error),
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.log_level(), "info");
        assert!(config.log_file_path().ends_with("niuma.log"));
    }

    #[test]
    fn test_expand_env_var() {
        // SAFETY: This is a test and we're only modifying our own test variable
        #[allow(unsafe_code)]
        unsafe {
            std::env::set_var("TEST_VAR", "test_value");
        }
        let result = expand_env_var("prefix_${TEST_VAR}_suffix");
        assert_eq!(result, "prefix_test_value_suffix");
        // SAFETY: This is a test and we're only modifying our own test variable
        #[allow(unsafe_code)]
        unsafe {
            std::env::remove_var("TEST_VAR");
        }
    }

    #[test]
    fn test_config_load_from_yaml() {
        let yaml = r#"
llm:
  default: "claude"
  providers:
    claude:
      api_key: "test-key"
      model: "claude-sonnet-4-6"

mcpServers:
  playwright:
    command: "npx"
    args: ["-y", "@playwright/mcp"]

storage:
  schedulesDir: "./data/schedules"
  cacheDir: "./data/cache"
  logsDir: "./data/logs"

logging:
  level: "debug"
"#;
        let config: Config = serde_yaml::from_str(yaml).expect("Should parse");
        assert_eq!(config.llm.default, "claude");
        assert_eq!(config.log_level(), "debug");
        assert!(config.mcp_servers.contains_key("playwright"));
    }

    #[test]
    fn test_mcp_server_config() {
        let yaml = r#"
command: "npx"
args: ["-y", "@playwright/mcp"]
env:
  HEADLESS: "true"
"#;
        let config: McpServerConfig = serde_yaml::from_str(yaml).expect("Should parse");
        assert_eq!(config.command, "npx");
        assert_eq!(config.args, vec!["-y", "@playwright/mcp"]);
        assert_eq!(config.env.get("HEADLESS"), Some(&"true".to_string()));
    }
}
