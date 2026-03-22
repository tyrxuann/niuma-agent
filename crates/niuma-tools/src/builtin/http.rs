//! HTTP request tool implementation.

use std::{collections::HashMap, time::Duration};

use async_trait::async_trait;
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::debug;

use crate::{Error, Result, Tool};

/// Default timeout for HTTP requests (30 seconds).
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Maximum timeout allowed (5 minutes).
const MAX_TIMEOUT_SECS: u64 = 300;

/// Maximum response body size (10 MB).
const MAX_RESPONSE_SIZE: usize = 10 * 1024 * 1024;

/// HTTP methods supported by the tool.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
enum HttpMethod {
    /// HTTP GET
    #[default]
    Get,
    /// HTTP POST
    Post,
    /// HTTP PUT
    Put,
    /// HTTP PATCH
    Patch,
    /// HTTP DELETE
    Delete,
    /// HTTP HEAD
    Head,
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Get => write!(f, "GET"),
            Self::Post => write!(f, "POST"),
            Self::Put => write!(f, "PUT"),
            Self::Patch => write!(f, "PATCH"),
            Self::Delete => write!(f, "DELETE"),
            Self::Head => write!(f, "HEAD"),
        }
    }
}

/// Input parameters for the `HTTP` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HttpInput {
    /// The URL to request.
    url: String,
    /// HTTP method (GET, POST, PUT, PATCH, DELETE, HEAD).
    #[serde(default)]
    method: HttpMethod,
    /// Request headers.
    #[serde(default)]
    headers: HashMap<String, String>,
    /// Request body (for POST, PUT, PATCH).
    #[serde(default)]
    body: Option<Value>,
    /// Raw request body as string (alternative to body).
    #[serde(default)]
    raw_body: Option<String>,
    /// Query parameters.
    #[serde(default)]
    query: HashMap<String, String>,
    /// Timeout in seconds.
    #[serde(default = "default_timeout")]
    timeout_secs: u64,
    /// Whether to follow redirects.
    #[serde(default = "default_follow_redirects")]
    follow_redirects: bool,
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_SECS
}

fn default_follow_redirects() -> bool {
    true
}

/// Output from an HTTP request.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HttpOutput {
    /// Whether the request was successful (completed, not necessarily 200).
    success: bool,
    /// HTTP status code.
    status_code: u16,
    /// HTTP status message.
    status_message: String,
    /// Response headers.
    headers: HashMap<String, String>,
    /// Response body (as string, truncated if too large).
    body: Option<String>,
    /// Duration of the request in milliseconds.
    duration_ms: u64,
    /// Final URL (after redirects).
    final_url: String,
}

/// A tool for making HTTP requests.
///
/// This tool supports various HTTP methods, headers, request bodies,
/// and configurable timeouts. It uses `reqwest` with `rustls` for TLS.
///
/// # Security
///
/// - Requests have configurable timeouts (default 30s, max 5 minutes)
/// - Response body is truncated if too large
/// - Redirects can be controlled
#[derive(Debug)]
pub struct HttpTool {
    /// HTTP client that follows redirects.
    client_follow_redirects: reqwest::Client,
    /// HTTP client that does not follow redirects.
    client_no_redirects: reqwest::Client,
}

impl Default for HttpTool {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpTool {
    /// Creates a new HTTP tool with default client configuration.
    ///
    /// # Panics
    ///
    /// Panics if the HTTP client cannot be created (should not happen with valid config).
    #[must_use]
    pub fn new() -> Self {
        let client_follow_redirects = reqwest::Client::builder()
            .timeout(Duration::from_secs(MAX_TIMEOUT_SECS))
            .user_agent("niuma-agent/0.1.0")
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .expect("failed to create HTTP client");

        let client_no_redirects = reqwest::Client::builder()
            .timeout(Duration::from_secs(MAX_TIMEOUT_SECS))
            .user_agent("niuma-agent/0.1.0")
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("failed to create HTTP client");

        Self {
            client_follow_redirects,
            client_no_redirects,
        }
    }

    /// Creates a new HTTP tool with custom clients.
    #[must_use]
    pub fn with_clients(
        client_follow_redirects: reqwest::Client,
        client_no_redirects: reqwest::Client,
    ) -> Self {
        Self {
            client_follow_redirects,
            client_no_redirects,
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
        let capped = std::cmp::min(timeout_secs, MAX_TIMEOUT_SECS);
        Ok(Duration::from_secs(capped))
    }

    /// Truncates response body if it exceeds the maximum size.
    fn truncate_body(body: String) -> String {
        if body.len() > MAX_RESPONSE_SIZE {
            let truncated_len = MAX_RESPONSE_SIZE;
            let mut truncated = body.chars().take(truncated_len).collect::<String>();
            truncated.push_str("\n... [response body truncated]");
            truncated
        } else {
            body
        }
    }

    /// Converts headers to a HashMap.
    fn headers_to_map(headers: &HeaderMap) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for (name, value) in headers {
            let value_str = value.to_str().unwrap_or("[binary data]");
            map.insert(name.to_string(), value_str.to_string());
        }
        map
    }

    /// Selects the appropriate client based on redirect preference.
    fn get_client(&self, follow_redirects: bool) -> &reqwest::Client {
        if follow_redirects {
            &self.client_follow_redirects
        } else {
            &self.client_no_redirects
        }
    }
}

#[async_trait]
impl Tool for HttpTool {
    fn name(&self) -> &str {
        "http"
    }

    fn description(&self) -> &str {
        "Makes HTTP requests to external URLs. Supports various HTTP methods, headers, request \
         bodies, and query parameters. Returns status code, headers, and response body."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to request",
                    "format": "uri"
                },
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"],
                    "description": "HTTP method (default: GET)"
                },
                "headers": {
                    "type": "object",
                    "additionalProperties": { "type": "string" },
                    "description": "Request headers"
                },
                "body": {
                    "description": "Request body as JSON (for POST, PUT, PATCH)"
                },
                "raw_body": {
                    "type": "string",
                    "description": "Raw request body as string (alternative to body)"
                },
                "query": {
                    "type": "object",
                    "additionalProperties": { "type": "string" },
                    "description": "Query parameters"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 30, max: 300)",
                    "minimum": 1,
                    "maximum": MAX_TIMEOUT_SECS
                },
                "follow_redirects": {
                    "type": "boolean",
                    "description": "Whether to follow redirects (default: true)"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let input: HttpInput =
            serde_json::from_value(args).map_err(|e| Error::InvalidArguments {
                tool: self.name().to_string(),
                message: e.to_string(),
            })?;

        debug!("Making {} request to: {}", input.method, input.url);

        let timeout_duration = self.validate_timeout(input.timeout_secs)?;
        let client = self.get_client(input.follow_redirects);

        let start = std::time::Instant::now();

        // Build the request
        let mut request_builder = match input.method {
            HttpMethod::Get => client.get(&input.url),
            HttpMethod::Post => client.post(&input.url),
            HttpMethod::Put => client.put(&input.url),
            HttpMethod::Patch => client.patch(&input.url),
            HttpMethod::Delete => client.delete(&input.url),
            HttpMethod::Head => client.head(&input.url),
        };

        // Set timeout (this will override the client's default timeout)
        request_builder = request_builder.timeout(timeout_duration);

        // Add headers
        for (key, value) in &input.headers {
            request_builder = request_builder.header(key, value);
        }

        // Add query parameters
        if !input.query.is_empty() {
            request_builder = request_builder.query(&input.query);
        }

        // Add body
        if let Some(ref raw_body) = input.raw_body {
            request_builder = request_builder.body(raw_body.clone());
        } else if let Some(ref body) = input.body {
            request_builder = request_builder.json(body);
        }

        // Execute the request
        let response = request_builder.send().await.map_err(|e| {
            if e.is_timeout() {
                Error::HttpRequest {
                    message: format!("request timed out after {} seconds", input.timeout_secs),
                }
            } else if e.is_connect() {
                Error::HttpRequest {
                    message: format!("connection failed: {e}"),
                }
            } else {
                Error::HttpRequest {
                    message: format!("request failed: {e}"),
                }
            }
        })?;

        let duration_ms = start.elapsed().as_millis() as u64;

        // Extract response information
        let status_code = response.status().as_u16();
        let status_message = response
            .status()
            .canonical_reason()
            .unwrap_or("Unknown")
            .to_string();
        let final_url = response.url().to_string();
        let headers = Self::headers_to_map(response.headers());

        // Get response body (except for HEAD requests)
        let body = if matches!(input.method, HttpMethod::Head) {
            None
        } else {
            let text = response.text().await.map_err(|e| Error::HttpRequest {
                message: format!("failed to read response body: {e}"),
            })?;
            Some(Self::truncate_body(text))
        };

        let output = HttpOutput {
            success: true,
            status_code,
            status_message,
            headers,
            body,
            duration_ms,
            final_url,
        };

        debug!(
            "HTTP request completed: {} (status: {}, duration: {}ms)",
            input.url, output.status_code, output.duration_ms
        );

        Ok(json!(output))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::Tool;

    #[test]
    fn test_tool_name() {
        let tool = HttpTool::new();
        assert_eq!(tool.name(), "http");
    }

    #[test]
    fn test_tool_description() {
        let tool = HttpTool::new();
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_input_schema() {
        let tool = HttpTool::new();
        let schema = tool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["url"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("url")));
    }

    #[test]
    fn test_http_method_display() {
        assert_eq!(format!("{}", HttpMethod::Get), "GET");
        assert_eq!(format!("{}", HttpMethod::Post), "POST");
        assert_eq!(format!("{}", HttpMethod::Put), "PUT");
    }

    #[test]
    fn test_validate_timeout_zero() {
        let tool = HttpTool::new();
        let result = tool.validate_timeout(0);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_timeout_too_large() {
        let tool = HttpTool::new();
        let result = tool.validate_timeout(MAX_TIMEOUT_SECS + 100);
        assert!(result.is_ok());
        let duration = result.expect("should succeed");
        assert_eq!(duration, Duration::from_secs(MAX_TIMEOUT_SECS));
    }

    #[test]
    fn test_truncate_body_normal() {
        let body = "Hello, World!".to_string();
        let truncated = HttpTool::truncate_body(body);
        assert_eq!(truncated, "Hello, World!");
    }

    #[test]
    fn test_truncate_body_too_large() {
        let body = "x".repeat(MAX_RESPONSE_SIZE + 1000);
        let truncated = HttpTool::truncate_body(body);
        assert!(truncated.len() < MAX_RESPONSE_SIZE + 100);
        assert!(truncated.ends_with("... [response body truncated]"));
    }

    #[tokio::test]
    async fn test_execute_invalid_url() {
        let tool = HttpTool::new();
        let args = json!({
            "url": "not-a-valid-url"
        });
        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    // Note: For real HTTP tests, consider using wiremock or similar
    // These tests would require network access which may not be available in all environments
}
