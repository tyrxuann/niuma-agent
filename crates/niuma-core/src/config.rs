//! Configuration types for niuma agent.
//!
//! This module provides configuration types for storage and other
//! shared settings used across niuma crates.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Default storage paths.
pub const DEFAULT_SCHEDULES_DIR: &str = "./data/schedules";
/// Default cache directory.
pub const DEFAULT_CACHE_DIR: &str = "./data/cache";
/// Default logs directory.
pub const DEFAULT_LOGS_DIR: &str = "./data/logs";

/// Storage configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageConfig {
    /// Directory for scheduled task definitions.
    #[serde(default = "default_schedules_dir")]
    pub schedules_dir: PathBuf,

    /// Directory for cached data (e.g., execution plans).
    #[serde(default = "default_cache_dir")]
    pub cache_dir: PathBuf,

    /// Directory for log files.
    #[serde(default = "default_logs_dir")]
    pub logs_dir: PathBuf,
}

fn default_schedules_dir() -> PathBuf {
    PathBuf::from(DEFAULT_SCHEDULES_DIR)
}

fn default_cache_dir() -> PathBuf {
    PathBuf::from(DEFAULT_CACHE_DIR)
}

fn default_logs_dir() -> PathBuf {
    PathBuf::from(DEFAULT_LOGS_DIR)
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            schedules_dir: default_schedules_dir(),
            cache_dir: default_cache_dir(),
            logs_dir: default_logs_dir(),
        }
    }
}

impl StorageConfig {
    /// Expands environment variables in all paths.
    pub fn expand_env(&mut self) {
        self.schedules_dir = PathBuf::from(expand_env_var(&self.schedules_dir.to_string_lossy()));
        self.cache_dir = PathBuf::from(expand_env_var(&self.cache_dir.to_string_lossy()));
        self.logs_dir = PathBuf::from(expand_env_var(&self.logs_dir.to_string_lossy()));
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_config_default() {
        let config = StorageConfig::default();
        assert_eq!(config.schedules_dir, PathBuf::from("./data/schedules"));
        assert_eq!(config.cache_dir, PathBuf::from("./data/cache"));
        assert_eq!(config.logs_dir, PathBuf::from("./data/logs"));
    }

    #[test]
    fn test_storage_config_deserialize() {
        let yaml = r#"
schedulesDir: "/var/niuma/schedules"
cacheDir: "/var/niuma/cache"
logsDir: "/var/niuma/logs"
"#;
        let config: StorageConfig = serde_yaml::from_str(yaml).expect("Should parse");
        assert_eq!(config.schedules_dir, PathBuf::from("/var/niuma/schedules"));
        assert_eq!(config.cache_dir, PathBuf::from("/var/niuma/cache"));
        assert_eq!(config.logs_dir, PathBuf::from("/var/niuma/logs"));
    }
}
