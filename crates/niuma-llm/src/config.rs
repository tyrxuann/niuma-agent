//! Configuration types for LLM providers.
//!
//! This module provides configuration types for setting up LLM providers,
//! including support for environment variable expansion.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// Configuration for all LLM providers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LLMConfig {
    /// The default provider to use.
    pub default: String,
    /// Provider-specific configurations.
    pub providers: HashMap<String, ProviderConfig>,
}

impl LLMConfig {
    /// Creates a new LLM configuration.
    #[must_use]
    pub fn new(default: impl Into<String>) -> Self {
        Self {
            default: default.into(),
            providers: HashMap::new(),
        }
    }

    /// Adds a provider configuration.
    #[must_use]
    pub fn with_provider(mut self, name: impl Into<String>, config: ProviderConfig) -> Self {
        self.providers.insert(name.into(), config);
        self
    }

    /// Gets the default provider configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the default provider is not configured.
    pub fn default_provider(&self) -> Result<&ProviderConfig> {
        self.providers
            .get(&self.default)
            .ok_or_else(|| Error::ProviderNotConfigured(self.default.clone()))
    }

    /// Gets a provider configuration by name.
    ///
    /// # Errors
    ///
    /// Returns an error if the provider is not configured.
    pub fn provider(&self, name: &str) -> Result<&ProviderConfig> {
        self.providers
            .get(name)
            .ok_or_else(|| Error::ProviderNotConfigured(name.to_string()))
    }

    /// Expands all environment variables in the configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if an environment variable reference cannot be expanded.
    pub fn expand_env(&mut self) -> Result<()> {
        for config in self.providers.values_mut() {
            config.expand_env()?;
        }
        Ok(())
    }
}

/// Configuration for a single LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// The API key for authentication.
    pub api_key: String,
    /// The model to use.
    #[serde(default)]
    pub model: Option<String>,
    /// The base URL for the API (optional, for custom endpoints).
    #[serde(default)]
    pub base_url: Option<String>,
    /// Additional provider-specific options.
    #[serde(default)]
    pub options: HashMap<String, serde_json::Value>,
}

impl ProviderConfig {
    /// Creates a new provider configuration.
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: None,
            base_url: None,
            options: HashMap::new(),
        }
    }

    /// Sets the model.
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Sets the base URL.
    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Adds an option.
    #[must_use]
    pub fn with_option(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.options.insert(key.into(), value);
        self
    }

    /// Gets the model, or returns a default.
    #[must_use]
    pub fn model_or_default(&self, default: &str) -> String {
        self.model.clone().unwrap_or_else(|| default.to_string())
    }

    /// Expands environment variables in the API key and base_url.
    ///
    /// Supports the syntax `${VAR_NAME}` which will be replaced with
    /// the value of the environment variable.
    ///
    /// # Errors
    ///
    /// Returns an error if the API key environment variable is not set.
    /// For base_url, if the environment variable is not set, it will be set to None.
    pub fn expand_env(&mut self) -> Result<()> {
        self.api_key = expand_env_var(&self.api_key)?;
        if let Some(ref url) = self.base_url {
            match expand_env_var(url) {
                Ok(expanded) => {
                    // If expanded URL is empty, set to None
                    if expanded.is_empty() {
                        self.base_url = None;
                    } else {
                        self.base_url = Some(expanded);
                    }
                }
                Err(_) => {
                    // Environment variable not found, set base_url to None
                    self.base_url = None;
                }
            }
        }
        Ok(())
    }
}

/// Expands environment variables in a string.
///
/// Supports the syntax `${VAR_NAME}` which will be replaced with
/// the value of the environment variable.
///
/// # Errors
///
/// Returns an error if the environment variable is not set.
fn expand_env_var(s: &str) -> Result<String> {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'

            let mut var_name = String::new();
            while let Some(&next_ch) = chars.peek() {
                if next_ch == '}' {
                    chars.next(); // consume '}'
                    break;
                }
                var_name.push(chars.next().expect("peeked char exists"));
            }

            if var_name.is_empty() {
                return Err(Error::InvalidEnvVarSyntax(
                    "empty variable name".to_string(),
                ));
            }

            let value = std::env::var(&var_name).map_err(|_| Error::EnvVarNotFound(var_name))?;
            result.push_str(&value);
        } else {
            result.push(ch);
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_config_creation() {
        let config = ProviderConfig::new("test-key").with_model("gpt-4");
        assert_eq!(config.api_key, "test-key");
        assert_eq!(config.model, Some("gpt-4".to_string()));
    }

    #[test]
    fn test_model_or_default() {
        let config = ProviderConfig::new("test-key");
        assert_eq!(config.model_or_default("default-model"), "default-model");

        let config = ProviderConfig::new("test-key").with_model("custom-model");
        assert_eq!(config.model_or_default("default-model"), "custom-model");
    }

    #[test]
    fn test_expand_env_var_no_vars() {
        let result = expand_env_var("simple-string").expect("Should succeed");
        assert_eq!(result, "simple-string");
    }

    #[test]
    fn test_expand_env_var_with_vars() {
        // SAFETY: This is a test function, and we're setting/removing a test-specific
        // environment variable that shouldn't affect other code.
        unsafe {
            std::env::set_var("TEST_API_KEY", "secret-key");
        }
        let result = expand_env_var("prefix-${TEST_API_KEY}-suffix").expect("Should succeed");
        assert_eq!(result, "prefix-secret-key-suffix");
        // SAFETY: Same as above - removing the test variable we just set.
        unsafe {
            std::env::remove_var("TEST_API_KEY");
        }
    }

    #[test]
    fn test_expand_env_var_missing() {
        let result = expand_env_var("${NONEXISTENT_VAR_12345}");
        assert!(matches!(result, Err(Error::EnvVarNotFound(_))));
    }

    #[test]
    fn test_llm_config() {
        let config = LLMConfig::new("claude")
            .with_provider(
                "claude",
                ProviderConfig::new("claude-key").with_model("claude-sonnet-4-6"),
            )
            .with_provider(
                "openai",
                ProviderConfig::new("openai-key").with_model("gpt-4o"),
            );

        assert_eq!(config.default, "claude");
        assert!(config.providers.contains_key("claude"));
        assert!(config.providers.contains_key("openai"));
    }
}
