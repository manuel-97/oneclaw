//! LLM Provider Trait — Unified interface for calling any LLM
//!
//! This is the abstraction that lets OneClaw talk to Ollama, OpenAI,
//! Anthropic, or any future LLM provider through a single interface.

use crate::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// The role of the message sender
    pub role: MessageRole,
    /// The text content of the message
    pub content: String,
}

/// The role of a participant in a conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// A system-level instruction message
    System,
    /// A message from the user
    User,
    /// A message from the assistant
    Assistant,
}

/// Request to an LLM provider
#[derive(Debug, Clone)]
pub struct LlmRequest {
    /// Conversation messages
    pub messages: Vec<ChatMessage>,
    /// Model to use (provider-specific, e.g., "llama3.2:1b", "gpt-4o-mini")
    pub model: String,
    /// Maximum tokens in response
    pub max_tokens: Option<u32>,
    /// Temperature (0.0 = deterministic, 1.0 = creative)
    pub temperature: Option<f32>,
}

impl LlmRequest {
    /// Create a simple single-turn request
    pub fn simple(model: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: prompt.into(),
            }],
            model: model.into(),
            max_tokens: None,
            temperature: None,
        }
    }

    /// Create with system prompt + user message
    pub fn with_system(
        model: impl Into<String>,
        system: impl Into<String>,
        user: impl Into<String>,
    ) -> Self {
        Self {
            messages: vec![
                ChatMessage { role: MessageRole::System, content: system.into() },
                ChatMessage { role: MessageRole::User, content: user.into() },
            ],
            model: model.into(),
            max_tokens: None,
            temperature: None,
        }
    }

    /// Set the maximum number of tokens in the response
    pub fn set_max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = Some(tokens);
        self
    }

    /// Set the sampling temperature for the response
    pub fn set_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }
}

/// Response from an LLM provider
#[derive(Debug, Clone)]
pub struct LlmResponse {
    /// The generated text
    pub content: String,
    /// Model that was used
    pub model: String,
    /// Token usage (if available)
    pub usage: Option<TokenUsage>,
    /// Response time in milliseconds
    pub latency_ms: u64,
}

/// Token usage statistics from an LLM response
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    /// The number of tokens in the prompt
    pub prompt_tokens: u32,
    /// The number of tokens in the completion
    pub completion_tokens: u32,
    /// The total number of tokens used
    pub total_tokens: u32,
}

/// LLM Provider Trait — The core abstraction for calling LLMs (async)
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Provider name (e.g., "ollama", "openai")
    fn name(&self) -> &str;

    /// Check if provider is available/reachable
    async fn is_available(&self) -> Result<bool>;

    /// List available models
    async fn list_models(&self) -> Result<Vec<String>>;

    /// Send a chat completion request (async)
    async fn chat(&self, request: &LlmRequest) -> Result<LlmResponse>;
}

/// NoopProvider — returns a fixed response (for testing)
pub struct NoopProvider;

#[async_trait]
impl LlmProvider for NoopProvider {
    fn name(&self) -> &str { "noop" }

    async fn is_available(&self) -> Result<bool> { Ok(true) }

    async fn list_models(&self) -> Result<Vec<String>> {
        Ok(vec!["noop-model".to_string()])
    }

    async fn chat(&self, request: &LlmRequest) -> Result<LlmResponse> {
        let user_msg = request.messages.iter()
            .rfind(|m| m.role == MessageRole::User)
            .map(|m| m.content.clone())
            .unwrap_or_default();

        Ok(LlmResponse {
            content: format!("[noop] Received: {}", &user_msg[..user_msg.len().min(100)]),
            model: "noop-model".into(),
            usage: Some(TokenUsage { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 }),
            latency_ms: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_noop_provider() {
        let provider = NoopProvider;
        assert_eq!(provider.name(), "noop");
        assert!(provider.is_available().await.unwrap());
        assert!(!provider.list_models().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_noop_chat() {
        let provider = NoopProvider;
        let request = LlmRequest::simple("noop", "Hello world");
        let response = provider.chat(&request).await.unwrap();
        assert!(response.content.contains("Hello world"));
        assert_eq!(response.latency_ms, 0);
    }

    #[test]
    fn test_request_builders() {
        let req = LlmRequest::with_system("model", "You are helpful", "Hi")
            .set_max_tokens(100)
            .set_temperature(0.7);
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, MessageRole::System);
        assert_eq!(req.max_tokens, Some(100));
        assert_eq!(req.temperature, Some(0.7));
    }
}
