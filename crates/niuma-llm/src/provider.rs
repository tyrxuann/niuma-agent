//! LLM provider trait definition.
//!
//! This module defines the [`LLMProvider`] trait that all LLM providers must implement.

use async_trait::async_trait;
use futures::Stream;

use crate::{
    Error, Result,
    types::{ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse},
};

/// A trait for LLM providers.
///
/// This trait defines the interface for interacting with various LLM providers.
/// All providers must implement this trait to be used by the agent.
///
/// # Object Safety
///
/// This trait is object-safe and can be used with `dyn LLMProvider`.
/// We use `async_trait` because the trait needs to support dynamic dispatch
/// (`Arc<dyn LLMProvider>`), and native async traits do not yet support this.
///
/// # Example
///
/// ```rust,ignore
/// use std::sync::Arc;
/// use niuma_llm::{LLMProvider, ChatCompletionRequest, Message};
///
/// async fn chat(provider: Arc<dyn LLMProvider>) -> Result<(), Box<dyn std::error::Error>> {
///     let request = ChatCompletionRequest::new(
///         "claude-sonnet-4-6",
///         vec![Message::user("Hello!")],
///     );
///     let response = provider.complete(&request).await?;
///     println!("{:?}", response.choices[0].message.content);
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait LLMProvider: Send + Sync + std::fmt::Debug {
    /// Sends a chat completion request and returns the response.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails due to network issues,
    /// authentication problems, rate limiting, or invalid responses.
    async fn complete(&self, request: &ChatCompletionRequest) -> Result<ChatCompletionResponse>;

    /// Sends a streaming chat completion request.
    ///
    /// Returns a stream of chunks that can be collected to form the complete response.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails to initiate or if the stream
    /// encounters an error during processing.
    ///
    /// # Default Implementation
    ///
    /// The default implementation returns a not-implemented error. Providers
    /// should override this if they support streaming.
    async fn stream_complete(
        &self,
        _request: &ChatCompletionRequest,
    ) -> Result<std::pin::Pin<Box<dyn Stream<Item = Result<ChatCompletionChunk>> + Send>>> {
        Err(Error::StreamingNotSupported)
    }

    /// Returns the name of this provider.
    fn name(&self) -> &str;

    /// Returns the default model for this provider.
    fn default_model(&self) -> &str;
}

/// A builder for creating LLM provider instances.
///
/// This trait provides a common interface for configuring and creating
/// provider instances with their specific configuration.
#[async_trait]
pub trait LLMProviderBuilder: Send + Sync {
    /// The provider type this builder creates.
    type Provider: LLMProvider;

    /// Creates a new builder with the API key.
    fn new(api_key: impl Into<String>) -> Self;

    /// Sets the model to use.
    fn with_model(self, model: impl Into<String>) -> Self;

    /// Sets the base URL for the API (for custom endpoints).
    fn with_base_url(self, base_url: impl Into<String>) -> Self;

    /// Builds and validates the provider.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    async fn build(self) -> Result<Self::Provider>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::StreamingNotSupported;
        assert_eq!(
            err.to_string(),
            "Streaming is not supported by this provider"
        );
    }
}
