//! Configuration management for niuma-cli.
//!
//! This module provides configuration loading and management, including
//! log file settings.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Default log directory.
const DEFAULT_LOGS_DIR: &str = "./data/logs";

/// Default log file name.
const DEFAULT_LOG_FILE: &str = "niuma.log";

/// Default log level.
const DEFAULT_LOG_LEVEL: &str = "info";

/// Application configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Logging configuration.
    #[serde(default)]
    pub logging: LoggingConfig,
}

impl Config {
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

        // Expand environment variables in paths
        config.logging.expand_env_vars();

        Ok(config)
    }

    /// Returns the log file path.
    #[must_use]
    pub fn log_file_path(&self) -> PathBuf {
        let dir = if self.logging.logs_dir.is_empty() {
            PathBuf::from(DEFAULT_LOGS_DIR)
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

/// Logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingConfig {
    /// Log level: trace, debug, info, warn, error.
    #[serde(default)]
    pub level: String,

    /// Directory for log files.
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
            level: String::new(),    // Will use DEFAULT_LOG_LEVEL
            logs_dir: String::new(), // Will use DEFAULT_LOGS_DIR
            log_file: String::new(), // Will use DEFAULT_LOG_FILE
            max_file_size_mb: default_max_file_size(),
            max_files: default_max_files(),
        }
    }
}

impl LoggingConfig {
    /// Expands environment variables in paths.
    fn expand_env_vars(&mut self) {
        self.logs_dir = expand_env_var(&self.logs_dir);
        self.log_file = expand_env_var(&self.log_file);
    }
}

/// Expands environment variables in a string.
///
/// Supports `${VAR_NAME}` syntax.
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
                // Continue from the same position since we replaced
                start = begin;
            } else {
                // Variable not found, skip past it
                start = end + 1;
            }
        } else {
            break;
        }
    }

    result
}

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
    fn test_log_file_path() {
        let mut config = Config::default();
        config.logging.logs_dir = "/var/log/niuma".to_string();
        config.logging.log_file = "app.log".to_string();
        assert_eq!(
            config.log_file_path(),
            PathBuf::from("/var/log/niuma/app.log")
        );
    }
}
