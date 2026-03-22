//! Anthropic Claude provider implementation.
//!
//! This module provides the [`ClaudeProvider`] for interacting with
//! Anthropic's Claude API.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

use crate::{
    Error, Result,
    provider::LLMProvider,
    types::{
        ChatChoice, ChatCompletionRequest, ChatCompletionResponse, Content, FinishReason, Message,
        Role, ToolCall, ToolType, Usage,
    },
};

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const DEFAULT_MODEL: &str = "claude-sonnet-4-6";
const DEFAULT_MAX_TOKENS: u32 = 4096;

/// Provider for Anthropic Claude API.
#[derive(Debug)]
pub struct ClaudeProvider {
    client: Client,
    api_key: String,
    model: String,
    base_url: Option<String>,
}

impl ClaudeProvider {
    /// Creates a new Claude provider.
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            model: DEFAULT_MODEL.to_string(),
            base_url: None,
        }
    }

    /// Sets the model to use.
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Sets a custom base URL.
    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Returns the API URL to use.
    fn api_url(&self) -> &str {
        self.base_url.as_deref().unwrap_or(ANTHROPIC_API_URL)
    }

    /// Converts a generic request to Claude's API format.
    fn build_request(&self, request: &ChatCompletionRequest) -> ClaudeRequest {
        let system = request.system.clone().or_else(|| {
            request
                .messages
                .iter()
                .find(|m| m.role == Role::System)
                .and_then(|m| {
                    if let Content::Text(text) = &m.content {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
        });

        let messages: Vec<ClaudeMessage> = request
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .filter_map(|m| self.convert_message(m))
            .collect();

        let tools: Vec<ClaudeTool> = request
            .tools
            .iter()
            .map(|t| ClaudeTool {
                name: t.function.name.clone(),
                description: Some(t.function.description.clone()),
                input_schema: t.function.parameters.clone(),
            })
            .collect();

        ClaudeRequest {
            model: request.model.clone(),
            max_tokens: request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            messages,
            system,
            tools: if tools.is_empty() { None } else { Some(tools) },
            stream: if request.stream { Some(true) } else { None },
            temperature: request.temperature,
        }
    }

    /// Converts a generic message to Claude's format.
    fn convert_message(&self, msg: &Message) -> Option<ClaudeMessage> {
        let role = match msg.role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "user", // Tool results come as user messages in Claude
            Role::System => return None, // System messages are handled separately
        };

        let content = match &msg.content {
            Content::Text(text) => {
                if msg.role == Role::Tool {
                    vec![ClaudeContent::ToolResult {
                        tool_use_id: msg.tool_call_id.clone().unwrap_or_default(),
                        content: text.clone(),
                    }]
                } else if text.is_empty() && msg.tool_calls.is_some() {
                    // Assistant message with only tool calls
                    let mut content = Vec::new();
                    for tc in msg.tool_calls.iter().flatten() {
                        content.push(ClaudeContent::ToolUse {
                            id: tc.id.clone(),
                            name: tc.function.name.clone(),
                            input: serde_json::from_str(&tc.function.arguments).unwrap_or_default(),
                        });
                    }
                    content
                } else {
                    vec![ClaudeContent::Text { text: text.clone() }]
                }
            }
            Content::Parts(parts) => parts
                .iter()
                .map(|p| match p {
                    crate::types::ContentPart::Text { text } => {
                        ClaudeContent::Text { text: text.clone() }
                    }
                    crate::types::ContentPart::ImageUrl { image_url } => ClaudeContent::Image {
                        source: ClaudeImageSource {
                            r#type: "url".to_string(),
                            url: image_url.url.clone(),
                        },
                    },
                })
                .collect(),
        };

        Some(ClaudeMessage {
            role: role.to_string(),
            content,
        })
    }
}

#[async_trait]
impl LLMProvider for ClaudeProvider {
    #[instrument(skip(self, request), fields(model = %request.model))]
    async fn complete(&self, request: &ChatCompletionRequest) -> Result<ChatCompletionResponse> {
        let claude_request = self.build_request(request);

        debug!("Sending Claude request: {:?}", claude_request);

        let response = self
            .client
            .post(self.api_url())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&claude_request)
            .send()
            .await
            .map_err(|e| Error::http(format!("Request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(handle_claude_error(status.as_u16(), &error_text));
        }

        let claude_response: ClaudeResponse = response
            .json()
            .await
            .map_err(|e| Error::parse(format!("Failed to parse response: {e}")))?;

        Ok(convert_response(claude_response))
    }

    fn name(&self) -> &str {
        "claude"
    }

    fn default_model(&self) -> &str {
        &self.model
    }
}

/// Handles Claude API errors.
fn handle_claude_error(status: u16, body: &str) -> Error {
    match status {
        401 => Error::AuthenticationError("Invalid API key".to_string()),
        429 => Error::RateLimitExceeded,
        408 => Error::Timeout,
        404 => Error::ModelNotFound("Model not found".to_string()),
        _ => {
            // Try to parse the error response
            if let Ok(error_response) = serde_json::from_str::<ClaudeErrorResponse>(body) {
                Error::api(error_response.error.message)
            } else {
                Error::api(format!("API error (status {status}): {body}"))
            }
        }
    }
}

/// Converts a Claude response to the generic format.
fn convert_response(response: ClaudeResponse) -> ChatCompletionResponse {
    let (content, tool_calls) = extract_content_and_tools(&response.content);

    let message = Message {
        role: Role::Assistant,
        content: Content::Text(content),
        tool_calls: if tool_calls.is_empty() {
            None
        } else {
            Some(tool_calls)
        },
        tool_call_id: None,
        name: None,
    };

    let finish_reason = match response.stop_reason.as_deref() {
        Some("end_turn") => FinishReason::Stop,
        Some("max_tokens") => FinishReason::Length,
        Some("tool_use") => FinishReason::ToolCall,
        Some("stop_sequence") => FinishReason::Stop,
        _ => FinishReason::Unknown,
    };

    ChatCompletionResponse {
        id: response.id,
        model: response.model,
        choices: vec![ChatChoice {
            index: 0,
            message,
            finish_reason,
        }],
        usage: Usage {
            prompt_tokens: response.usage.input_tokens,
            completion_tokens: response.usage.output_tokens,
            total_tokens: response.usage.input_tokens + response.usage.output_tokens,
        },
    }
}

/// Extracts text content and tool calls from Claude content blocks.
fn extract_content_and_tools(content: &[ClaudeContent]) -> (String, Vec<ToolCall>) {
    let mut text = String::new();
    let mut tool_calls = Vec::new();

    for block in content {
        match block {
            ClaudeContent::Text { text: t } => {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(t);
            }
            ClaudeContent::ToolUse { id, name, input } => {
                tool_calls.push(ToolCall {
                    id: id.clone(),
                    tool_type: ToolType::Function,
                    function: crate::types::FunctionCall {
                        name: name.clone(),
                        arguments: serde_json::to_string(input).unwrap_or_default(),
                    },
                });
            }
            _ => {}
        }
    }

    (text, tool_calls)
}

// Claude API request/response types

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct ClaudeRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ClaudeMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ClaudeTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize)]
struct ClaudeMessage {
    role: String,
    content: Vec<ClaudeContent>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(variant_size_differences)]
enum ClaudeContent {
    Text {
        text: String,
    },
    Image {
        source: ClaudeImageSource,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct ClaudeImageSource {
    r#type: String,
    url: String,
}

#[derive(Debug, Serialize)]
struct ClaudeTool {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    input_schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ClaudeResponse {
    id: String,
    #[allow(dead_code)]
    r#type: String,
    #[allow(dead_code)]
    role: String,
    model: String,
    content: Vec<ClaudeContent>,
    stop_reason: Option<String>,
    usage: ClaudeUsage,
}

#[derive(Debug, Deserialize)]
struct ClaudeUsage {
    input_tokens: u64,
    output_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct ClaudeErrorResponse {
    error: ClaudeError,
}

#[derive(Debug, Deserialize)]
struct ClaudeError {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    error_type: String,
    message: String,
}

/// Builder for creating a Claude provider.
#[derive(Debug)]
pub struct ClaudeBuilder {
    api_key: String,
    model: Option<String>,
    base_url: Option<String>,
}

impl ClaudeBuilder {
    /// Creates a new builder.
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: None,
            base_url: None,
        }
    }

    /// Sets the model.
    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Sets the base URL.
    #[must_use]
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Builds the provider.
    #[must_use]
    pub fn build(self) -> ClaudeProvider {
        let mut provider = ClaudeProvider::new(self.api_key);
        if let Some(model) = self.model {
            provider = provider.with_model(model);
        }
        if let Some(base_url) = self.base_url {
            provider = provider.with_base_url(base_url);
        }
        provider
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_provider_creation() {
        let provider = ClaudeProvider::new("test-key").with_model("claude-opus-4");
        assert_eq!(provider.name(), "claude");
        assert_eq!(provider.default_model(), "claude-opus-4");
    }

    #[test]
    fn test_claude_builder() {
        let provider = ClaudeBuilder::new("test-key")
            .model("claude-sonnet-4-6")
            .base_url("https://custom.api.com")
            .build();
        assert_eq!(provider.name(), "claude");
    }

    #[test]
    fn test_build_request() {
        let provider = ClaudeProvider::new("test-key");
        let request = ChatCompletionRequest::new(
            "claude-sonnet-4-6",
            vec![Message::system("You are helpful"), Message::user("Hello")],
        );

        let claude_req = provider.build_request(&request);
        assert_eq!(claude_req.model, "claude-sonnet-4-6");
        assert_eq!(claude_req.system, Some("You are helpful".to_string()));
        assert_eq!(claude_req.messages.len(), 1);
    }
}
