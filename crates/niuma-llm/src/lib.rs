//! LLM integration layer for niuma agent.
//!
//! This crate provides abstractions and implementations for interacting
//! with various LLM providers, including Anthropic Claude and OpenAI.
//!
//! # Architecture
//!
//! The crate is organized around the [`LLMProvider`] trait, which defines
//! the interface for all LLM providers. Each provider implements this trait
//! to provide a consistent API for making chat completions.
//!
//! # Providers
//!
//! - [`ClaudeProvider`] - Anthropic Claude API
//! - [`OpenAIProvider`] - OpenAI Chat Completions API
//!
//! # Example
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use niuma_llm::{LLMProvider, ClaudeProvider, ChatCompletionRequest, Message};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let provider = Arc::new(ClaudeProvider::new("your-api-key"));
//!     let request = ChatCompletionRequest::new(
//!         "claude-sonnet-4-6",
//!         vec![Message::user("Hello!")],
//!     );
//!     let response = provider.complete(&request).await?;
//!     println!("{:?}", response.choices[0].message.content);
//!     Ok(())
//! }
//! ```

#![warn(missing_docs)]
#![warn(rust_2024_compatibility)]
#![warn(missing_debug_implementations)]

pub mod claude;
pub mod config;
pub mod error;
pub mod openai;
pub mod provider;
pub mod types;

pub use claude::{ClaudeBuilder, ClaudeProvider};
pub use config::{LLMConfig, ProviderConfig};
pub use error::{Error, Result};
pub use openai::{OpenAIBuilder, OpenAIProvider};
pub use provider::LLMProvider;
pub use types::{
    ChatChoice, ChatChunkChoice, ChatCompletionChunk, ChatCompletionRequest,
    ChatCompletionResponse, ChatDelta, Content, ContentPart, FinishReason, FunctionCall,
    FunctionCallDelta, FunctionDefinition, ImageUrl, Message, Role, ToolCall, ToolCallBatch,
    ToolCallDelta, ToolDefinition, ToolType, Usage,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::generic("test error");
        assert_eq!(err.to_string(), "test error");
    }

    #[test]
    fn test_provider_name() {
        let claude = ClaudeProvider::new("test");
        assert_eq!(claude.name(), "claude");

        let openai = OpenAIProvider::new("test");
        assert_eq!(openai.name(), "openai");
    }
}
