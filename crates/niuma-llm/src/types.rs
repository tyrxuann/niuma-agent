//! Core types for LLM interactions.
//!
//! This module defines the common types used across all LLM providers,
//! including messages, chat completions, and tool definitions.

use serde::{Deserialize, Serialize};

/// A message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// The role of the message author.
    pub role: Role,
    /// The content of the message.
    pub content: Content,
    /// Optional tool calls for assistant messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Optional tool call ID for tool response messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Optional name for user messages (used in tool/function context).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Message {
    /// Creates a new system message.
    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: Content::Text(content.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    /// Creates a new user message.
    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: Content::Text(content.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    /// Creates a new assistant message.
    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: Content::Text(content.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    /// Creates a new assistant message with tool calls.
    #[must_use]
    pub fn assistant_with_tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            content: Content::Text(String::new()),
            tool_calls: Some(tool_calls),
            tool_call_id: None,
            name: None,
        }
    }

    /// Creates a new tool response message.
    #[must_use]
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: Content::Text(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
            name: None,
        }
    }
}

/// The role of a message author.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System message for instructions.
    System,
    /// User message.
    User,
    /// Assistant message.
    Assistant,
    /// Tool response message.
    Tool,
}

/// Content of a message, which can be text or multipart.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Content {
    /// Simple text content.
    Text(String),
    /// Multipart content with multiple parts.
    Parts(Vec<ContentPart>),
}

impl Default for Content {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

/// A part of multipart content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    /// Text content part.
    Text {
        /// The text content.
        text: String,
    },
    /// Image content part.
    ImageUrl {
        /// The image URL.
        image_url: ImageUrl,
    },
}

/// An image URL for image content parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    /// The URL of the image.
    pub url: String,
    /// Optional detail level for the image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// A tool call from the assistant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// The ID of the tool call.
    pub id: String,
    /// The type of the tool (always "function" for now).
    #[serde(rename = "type")]
    pub tool_type: ToolType,
    /// The function call details.
    pub function: FunctionCall,
}

/// The type of tool being called.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolType {
    /// A function call.
    Function,
}

/// A function call within a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// The name of the function to call.
    pub name: String,
    /// The arguments to pass to the function (JSON string).
    pub arguments: String,
}

/// Definition of a tool that can be called by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// The type of the tool (always "function" for now).
    #[serde(rename = "type")]
    pub tool_type: ToolType,
    /// The function definition.
    pub function: FunctionDefinition,
}

impl ToolDefinition {
    /// Creates a new tool definition.
    #[must_use]
    pub fn function(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            tool_type: ToolType::Function,
            function: FunctionDefinition {
                name: name.into(),
                description: description.into(),
                parameters,
            },
        }
    }
}

/// Definition of a function that can be called by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// The name of the function.
    pub name: String,
    /// A description of what the function does.
    pub description: String,
    /// The JSON schema for the function parameters.
    pub parameters: serde_json::Value,
}

/// A chat completion request.
#[derive(Debug, Clone)]
pub struct ChatCompletionRequest {
    /// The model to use for completion.
    pub model: String,
    /// The messages in the conversation.
    pub messages: Vec<Message>,
    /// The tools available for the LLM to call.
    pub tools: Vec<ToolDefinition>,
    /// Whether to stream the response.
    pub stream: bool,
    /// The maximum tokens to generate.
    pub max_tokens: Option<u32>,
    /// The temperature for sampling.
    pub temperature: Option<f32>,
    /// The system prompt (for providers that support it separately).
    pub system: Option<String>,
}

impl ChatCompletionRequest {
    /// Creates a new chat completion request.
    #[must_use]
    pub fn new(model: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            model: model.into(),
            messages,
            tools: Vec::new(),
            stream: false,
            max_tokens: None,
            temperature: None,
            system: None,
        }
    }

    /// Adds tools to the request.
    #[must_use]
    pub fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    /// Sets whether to stream the response.
    #[must_use]
    pub fn with_stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    /// Sets the maximum tokens to generate.
    #[must_use]
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Sets the temperature for sampling.
    #[must_use]
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Sets the system prompt.
    #[must_use]
    pub fn with_system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }
}

/// A chat completion response.
#[derive(Debug, Clone)]
pub struct ChatCompletionResponse {
    /// The ID of the completion.
    pub id: String,
    /// The model used for the completion.
    pub model: String,
    /// The generated choices.
    pub choices: Vec<ChatChoice>,
    /// Usage statistics.
    pub usage: Usage,
}

/// A choice in a chat completion response.
#[derive(Debug, Clone)]
pub struct ChatChoice {
    /// The index of the choice.
    pub index: u32,
    /// The message generated.
    pub message: Message,
    /// The finish reason.
    pub finish_reason: FinishReason,
}

/// The reason the completion finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishReason {
    /// The completion stopped normally.
    Stop,
    /// The maximum tokens were reached.
    Length,
    /// A tool was called.
    ToolCall,
    /// Content was filtered.
    ContentFilter,
    /// Unknown reason.
    Unknown,
}

/// Usage statistics for a completion.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    /// The number of tokens in the prompt.
    pub prompt_tokens: u64,
    /// The number of tokens in the completion.
    pub completion_tokens: u64,
    /// The total number of tokens.
    pub total_tokens: u64,
}

/// A streaming chunk from a chat completion.
#[derive(Debug, Clone)]
pub struct ChatCompletionChunk {
    /// The ID of the completion.
    pub id: String,
    /// The model used for the completion.
    pub model: String,
    /// The delta choices.
    pub choices: Vec<ChatChunkChoice>,
}

/// A choice in a streaming chunk.
#[derive(Debug, Clone)]
pub struct ChatChunkChoice {
    /// The index of the choice.
    pub index: u32,
    /// The delta content.
    pub delta: ChatDelta,
    /// The finish reason (only present in the final chunk).
    pub finish_reason: Option<FinishReason>,
}

/// Delta content in a streaming chunk.
#[derive(Debug, Clone, Default)]
pub struct ChatDelta {
    /// The role (only in the first chunk).
    pub role: Option<Role>,
    /// The text content delta.
    pub content: Option<String>,
    /// Tool calls delta.
    pub tool_calls: Option<Vec<ToolCallDelta>>,
}

/// A tool call delta in a streaming chunk.
#[derive(Debug, Clone)]
pub struct ToolCallDelta {
    /// The index of the tool call.
    pub index: u32,
    /// The ID of the tool call (only in the first chunk).
    pub id: Option<String>,
    /// The type of the tool.
    pub tool_type: Option<ToolType>,
    /// The function call delta.
    pub function: Option<FunctionCallDelta>,
}

/// A function call delta in a streaming chunk.
#[derive(Debug, Clone)]
pub struct FunctionCallDelta {
    /// The name of the function (only in the first chunk).
    pub name: Option<String>,
    /// The arguments delta (JSON string fragment).
    pub arguments: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = Message::user("Hello");
        assert_eq!(msg.role, Role::User);
        assert!(matches!(msg.content, Content::Text(ref s) if s == "Hello"));

        let msg = Message::system("You are helpful");
        assert_eq!(msg.role, Role::System);

        let msg = Message::assistant("Hi there!");
        assert_eq!(msg.role, Role::Assistant);
    }

    #[test]
    fn test_message_serialization() {
        let msg = Message::user("Hello");
        let json = serde_json::to_string(&msg).expect("Failed to serialize");
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"Hello\""));
    }

    #[test]
    fn test_tool_definition() {
        let tool = ToolDefinition::function(
            "get_weather",
            "Get the current weather",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "location": {"type": "string"}
                }
            }),
        );
        assert_eq!(tool.function.name, "get_weather");
    }
}
