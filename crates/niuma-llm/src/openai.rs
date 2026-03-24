//! OpenAI provider implementation.
//!
//! This module provides the [`OpenAIProvider`] for interacting with
//! OpenAI's Chat Completions API.

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

const OPENAI_API_URL: &str = "https://api.openai.com/v1/chat/completions";
const DEFAULT_MODEL: &str = "gpt-4o";

/// Provider for OpenAI API.
#[derive(Debug)]
pub struct OpenAIProvider {
    client: Client,
    api_key: String,
    model: String,
    base_url: Option<String>,
}

impl OpenAIProvider {
    /// Creates a new OpenAI provider.
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

    /// Sets a custom base URL (for Azure or other compatible APIs).
    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Returns the API URL to use.
    fn api_url(&self) -> &str {
        self.base_url.as_deref().unwrap_or(OPENAI_API_URL)
    }

    /// Converts a generic request to OpenAI's API format.
    fn build_request(&self, request: &ChatCompletionRequest) -> OpenAIRequest {
        let messages: Vec<OpenAIMessage> = request.messages.iter().map(convert_message).collect();

        let tools: Vec<OpenAITool> = request
            .tools
            .iter()
            .map(|t| OpenAITool {
                r#type: "function".to_string(),
                function: OpenAIFunction {
                    name: t.function.name.clone(),
                    description: Some(t.function.description.clone()),
                    parameters: Some(t.function.parameters.clone()),
                },
            })
            .collect();

        OpenAIRequest {
            model: request.model.clone(),
            messages,
            tools: if tools.is_empty() { None } else { Some(tools) },
            stream: if request.stream { Some(true) } else { None },
            max_tokens: request.max_tokens,
            temperature: request.temperature,
        }
    }
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    #[instrument(skip(self, request), fields(model = %request.model))]
    async fn complete(&self, request: &ChatCompletionRequest) -> Result<ChatCompletionResponse> {
        let openai_request = self.build_request(request);

        debug!("Sending OpenAI request: {:?}", openai_request);

        let response = self
            .client
            .post(self.api_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&openai_request)
            .send()
            .await
            .map_err(|e| Error::http(format!("Request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(handle_openai_error(status.as_u16(), &error_text));
        }

        let openai_response: OpenAIResponse = response
            .json()
            .await
            .map_err(|e| Error::parse(format!("Failed to parse response: {e}")))?;

        Ok(convert_response(openai_response))
    }

    fn name(&self) -> &str {
        "openai"
    }

    fn default_model(&self) -> &str {
        &self.model
    }
}

/// Handles OpenAI API errors.
fn handle_openai_error(status: u16, body: &str) -> Error {
    match status {
        401 => Error::AuthenticationError("Invalid API key".to_string()),
        429 => Error::RateLimitExceeded,
        408 => Error::Timeout,
        404 => Error::ModelNotFound("Model not found".to_string()),
        _ => {
            // Try to parse the error response
            if let Ok(error_response) = serde_json::from_str::<OpenAIErrorResponse>(body) {
                let msg = error_response
                    .error
                    .message
                    .or_else(|| error_response.error.code.clone())
                    .unwrap_or_else(|| "Unknown error".to_string());
                Error::api(msg)
            } else {
                Error::api(format!("API error (status {status}): {body}"))
            }
        }
    }
}

/// Converts a generic message to OpenAI's format.
fn convert_message(msg: &Message) -> OpenAIMessage {
    let role = match msg.role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    };

    OpenAIMessage {
        role: role.to_string(),
        content: match &msg.content {
            Content::Text(text) => {
                if text.is_empty() {
                    None
                } else {
                    Some(OpenAIContent::Text(text.clone()))
                }
            }
            Content::Parts(parts) => Some(OpenAIContent::Parts(
                parts
                    .iter()
                    .map(|p| match p {
                        crate::types::ContentPart::Text { text } => OpenAIContentPart::Text {
                            r#type: TextType::new(),
                            text: text.clone(),
                        },
                        crate::types::ContentPart::ImageUrl { image_url } => {
                            OpenAIContentPart::ImageUrl {
                                r#type: ImageUrlType::new(),
                                image_url: OpenAIImageUrl {
                                    url: image_url.url.clone(),
                                    detail: image_url.detail.clone(),
                                },
                            }
                        }
                    })
                    .collect(),
            )),
        },
        tool_calls: msg.tool_calls.as_ref().map(|tcs| {
            tcs.iter()
                .map(|tc| OpenAIToolCall {
                    id: tc.id.clone(),
                    r#type: "function".to_string(),
                    function: OpenAIFunctionCall {
                        name: tc.function.name.clone(),
                        arguments: tc.function.arguments.clone(),
                    },
                })
                .collect()
        }),
        tool_call_id: msg.tool_call_id.clone(),
        name: msg.name.clone(),
    }
}

/// Converts an OpenAI response to the generic format.
fn convert_response(response: OpenAIResponse) -> ChatCompletionResponse {
    let choices: Vec<ChatChoice> = response
        .choices
        .into_iter()
        .map(|c| {
            let message = Message {
                role: Role::Assistant,
                content: c.message.content.map(Content::Text).unwrap_or_default(),
                tool_calls: c.message.tool_calls.map(|tcs| {
                    tcs.into_iter()
                        .map(|tc| ToolCall {
                            id: tc.id,
                            tool_type: ToolType::Function,
                            function: crate::types::FunctionCall {
                                name: tc.function.name,
                                arguments: tc.function.arguments,
                            },
                        })
                        .collect()
                }),
                tool_call_id: None,
                name: None,
            };

            let finish_reason = match c.finish_reason.as_deref() {
                Some("stop") => FinishReason::Stop,
                Some("length") => FinishReason::Length,
                Some("tool_calls") => FinishReason::ToolCall,
                Some("content_filter") => FinishReason::ContentFilter,
                _ => FinishReason::Unknown,
            };

            ChatChoice {
                index: c.index,
                message,
                finish_reason,
            }
        })
        .collect();

    ChatCompletionResponse {
        id: response.id,
        model: response.model,
        choices,
        usage: response
            .usage
            .map(|u| Usage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
            })
            .unwrap_or_default(),
    }
}

// OpenAI API request/response types

#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAITool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize)]
struct OpenAIMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<OpenAIContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum OpenAIContent {
    Text(String),
    Parts(Vec<OpenAIContentPart>),
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum OpenAIContentPart {
    Text {
        r#type: TextType,
        text: String,
    },
    ImageUrl {
        r#type: ImageUrlType,
        image_url: OpenAIImageUrl,
    },
}

#[derive(Debug, Serialize)]
struct TextType {
    r#type: &'static str,
}

impl TextType {
    const fn new() -> Self {
        Self { r#type: "text" }
    }
}

#[derive(Debug, Serialize)]
struct ImageUrlType {
    r#type: &'static str,
}

impl ImageUrlType {
    const fn new() -> Self {
        Self {
            r#type: "image_url",
        }
    }
}

#[derive(Debug, Serialize)]
struct OpenAIImageUrl {
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpenAITool {
    r#type: String,
    function: OpenAIFunction,
}

#[derive(Debug, Serialize)]
struct OpenAIFunction {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct OpenAIToolCall {
    id: String,
    r#type: String,
    function: OpenAIFunctionCall,
}

#[derive(Debug, Serialize)]
struct OpenAIFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    id: String,
    #[allow(dead_code)]
    object: String,
    model: String,
    choices: Vec<OpenAIChoice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    index: u32,
    message: OpenAIResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponseMessage {
    #[allow(dead_code)]
    role: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAIResponseToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponseToolCall {
    id: String,
    #[allow(dead_code)]
    r#type: String,
    function: OpenAIResponseFunctionCall,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponseFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct OpenAIErrorResponse {
    error: OpenAIError,
}

#[derive(Debug, Deserialize)]
struct OpenAIError {
    message: Option<String>,
    code: Option<String>,
}

/// Builder for creating an OpenAI provider.
#[derive(Debug)]
pub struct OpenAIBuilder {
    api_key: String,
    model: Option<String>,
    base_url: Option<String>,
}

impl OpenAIBuilder {
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
    pub fn build(self) -> OpenAIProvider {
        let mut provider = OpenAIProvider::new(self.api_key);
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
    fn test_openai_provider_creation() {
        let provider = OpenAIProvider::new("test-key").with_model("gpt-4-turbo");
        assert_eq!(provider.name(), "openai");
        assert_eq!(provider.default_model(), "gpt-4-turbo");
    }

    #[test]
    fn test_openai_builder() {
        let provider = OpenAIBuilder::new("test-key")
            .model("gpt-4o")
            .base_url("https://custom.api.com")
            .build();
        assert_eq!(provider.name(), "openai");
    }

    #[test]
    fn test_build_request() {
        let provider = OpenAIProvider::new("test-key");
        let request = ChatCompletionRequest::new(
            "gpt-4o",
            vec![Message::system("You are helpful"), Message::user("Hello")],
        );

        let openai_req = provider.build_request(&request);
        assert_eq!(openai_req.model, "gpt-4o");
        assert_eq!(openai_req.messages.len(), 2);
    }

    #[test]
    fn test_convert_message() {
        let msg = Message::user("Hello");
        let openai_msg = convert_message(&msg);
        assert_eq!(openai_msg.role, "user");

        let msg = Message::tool("call_123", "result");
        let openai_msg = convert_message(&msg);
        assert_eq!(openai_msg.role, "tool");
        assert_eq!(openai_msg.tool_call_id, Some("call_123".to_string()));
    }
}
