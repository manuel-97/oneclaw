//! Provider trait — foundation for multi-model support (v1.5)
//!
//! OneClaw Provider Strategy (inspired by ZeroClaw, focused like OneClaw):
//!
//! Tier 1 (Must):  Claude (Anthropic), GPT-4 (OpenAI), Gemini (Google)
//! Tier 2 (Pick 2): DeepSeek, Groq
//! Local:           Ollama (self-hosted, offline capable on edge)
//!
//! All providers implement the same trait. Swap via config.
//! Fallback chain: primary -> secondary -> local (if configured).
//!
//! This is the unified provider interface (v1.5). All LLM backends implement
//! the `Provider` trait. Sync-friendly, designed for edge/IoT simplicity.

use crate::error::Result;

/// A message in a provider conversation (v1.5 type)
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// Role of the message sender
    pub role: MessageRole,
    /// Text content
    pub content: String,
}

/// Role of a participant in a conversation
#[derive(Debug, Clone, PartialEq)]
pub enum MessageRole {
    /// System-level instruction
    System,
    /// User message
    User,
    /// Assistant response
    Assistant,
}

/// Response from an LLM provider
#[derive(Debug, Clone)]
pub struct ProviderResponse {
    /// The generated text
    pub content: String,
    /// Which provider actually served the response
    pub provider_id: &'static str,
    /// Token usage (if available)
    pub usage: Option<TokenUsage>,
}

/// Token usage statistics
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    /// Tokens in the prompt
    pub prompt_tokens: u32,
    /// Tokens in the completion
    pub completion_tokens: u32,
    /// Total tokens used
    pub total_tokens: u32,
}

/// Configuration for a provider instance
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// Provider identifier: "anthropic", "openai", "google", "deepseek", "groq", "ollama"
    pub provider_id: String,
    /// API endpoint (default per provider, override for custom/proxy)
    pub endpoint: Option<String>,
    /// API key (None for local providers like Ollama)
    pub api_key: Option<String>,
    /// Model name: "claude-sonnet-4-20250514", "gpt-4o", "gemini-2.0-flash", etc.
    pub model: String,
    /// Max tokens for response
    pub max_tokens: u32,
    /// Temperature (0.0 - 1.0)
    pub temperature: f32,
}

/// Core provider trait — every LLM backend implements this
///
/// Design principles (learned from ZeroClaw, adapted for OneClaw):
/// - Simple: only 2 chat methods (single message + history)
/// - Sync-friendly: returns Result, not Stream (v2.0 may add streaming)
/// - No lock-in: OpenAI-compatible API format as common denominator
pub trait Provider: Send + Sync {
    /// Provider identifier (e.g., "anthropic", "openai", "ollama")
    fn id(&self) -> &'static str;

    /// Send a single message with system prompt
    /// Used for: simple queries, alert generation, classification
    fn chat(&self, system: &str, user_message: &str) -> Result<ProviderResponse>;

    /// Send a conversation with history
    /// Used for: multi-turn conversations, context-aware responses
    fn chat_with_history(
        &self,
        system: &str,
        messages: &[ChatMessage],
    ) -> Result<ProviderResponse>;

    /// Health check — can this provider respond right now?
    /// Used for: fallback chain decisions, status reporting
    fn is_available(&self) -> bool;

    /// Human-readable name for display
    fn display_name(&self) -> &str;
}

/// Provider that wraps another with retry logic
/// Inspired by ZeroClaw's ReliableProvider pattern
pub struct ReliableProvider<P: Provider> {
    inner: P,
    max_retries: u32,
}

impl<P: Provider> ReliableProvider<P> {
    /// Create a new ReliableProvider wrapping an inner provider
    pub fn new(inner: P, max_retries: u32) -> Self {
        Self { inner, max_retries }
    }
}

impl<P: Provider> Provider for ReliableProvider<P> {
    fn id(&self) -> &'static str { self.inner.id() }
    fn display_name(&self) -> &str { self.inner.display_name() }
    fn is_available(&self) -> bool { self.inner.is_available() }

    fn chat(&self, system: &str, user_message: &str) -> Result<ProviderResponse> {
        let mut last_err = None;
        for attempt in 0..=self.max_retries {
            match self.inner.chat(system, user_message) {
                Ok(response) => return Ok(response),
                Err(e) => {
                    tracing::warn!(
                        provider = self.inner.id(),
                        attempt = attempt + 1,
                        error = %e,
                        "Provider call failed, retrying"
                    );
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.expect("at least one attempt was made"))
    }

    fn chat_with_history(
        &self,
        system: &str,
        messages: &[ChatMessage],
    ) -> Result<ProviderResponse> {
        let mut last_err = None;
        for attempt in 0..=self.max_retries {
            match self.inner.chat_with_history(system, messages) {
                Ok(response) => return Ok(response),
                Err(e) => {
                    tracing::warn!(
                        provider = self.inner.id(),
                        attempt = attempt + 1,
                        error = %e,
                        "Provider call failed, retrying"
                    );
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.expect("at least one attempt was made"))
    }
}

/// Fallback chain — try providers in order until one succeeds
/// OneClaw strategy: primary (cloud) -> secondary (cloud) -> local (Ollama)
pub struct FallbackChain {
    providers: Vec<Box<dyn Provider>>,
}

impl FallbackChain {
    /// Create a new fallback chain from ordered providers
    pub fn new(providers: Vec<Box<dyn Provider>>) -> Self {
        Self { providers }
    }

    /// Number of providers in the chain
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// Whether the chain has no providers
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    /// Provider info string: "anthropic → openai → ollama" (for status display)
    pub fn provider_info(&self) -> String {
        if self.providers.is_empty() {
            "(empty)".into()
        } else {
            self.providers.iter()
                .map(|p| p.id())
                .collect::<Vec<_>>()
                .join(" → ")
        }
    }
}

impl Provider for FallbackChain {
    fn id(&self) -> &'static str { "fallback-chain" }
    fn display_name(&self) -> &str { "Fallback Chain" }

    fn is_available(&self) -> bool {
        self.providers.iter().any(|p| p.is_available())
    }

    fn chat(&self, system: &str, user_message: &str) -> Result<ProviderResponse> {
        for provider in &self.providers {
            if !provider.is_available() {
                tracing::debug!(provider = provider.id(), "Skipping unavailable provider");
                continue;
            }
            match provider.chat(system, user_message) {
                Ok(response) => {
                    tracing::info!(provider = provider.id(), "Provider responded");
                    return Ok(response);
                }
                Err(e) => {
                    tracing::warn!(
                        provider = provider.id(),
                        error = %e,
                        "Provider failed, trying next"
                    );
                }
            }
        }
        Err(crate::error::OneClawError::Provider(
            "All providers in fallback chain failed".into()
        ))
    }

    fn chat_with_history(
        &self,
        system: &str,
        messages: &[ChatMessage],
    ) -> Result<ProviderResponse> {
        for provider in &self.providers {
            if !provider.is_available() { continue; }
            match provider.chat_with_history(system, messages) {
                Ok(response) => return Ok(response),
                Err(e) => {
                    tracing::warn!(
                        provider = provider.id(),
                        error = %e,
                        "Provider failed, trying next in chain"
                    );
                }
            }
        }
        Err(crate::error::OneClawError::Provider(
            "All providers in fallback chain failed".into()
        ))
    }
}

/// NoopProvider for testing — always succeeds with a fixed response
pub struct NoopTestProvider {
    available: bool,
}

impl NoopTestProvider {
    /// Create a NoopTestProvider that is available
    pub fn available() -> Self { Self { available: true } }
    /// Create a NoopTestProvider that is NOT available
    pub fn unavailable() -> Self { Self { available: false } }
}

impl Provider for NoopTestProvider {
    fn id(&self) -> &'static str { "noop-test" }
    fn display_name(&self) -> &str { "Noop Test Provider" }
    fn is_available(&self) -> bool { self.available }

    fn chat(&self, _system: &str, user_message: &str) -> Result<ProviderResponse> {
        Ok(ProviderResponse {
            content: format!("[noop-test] {}", user_message),
            provider_id: "noop-test",
            usage: Some(TokenUsage::default()),
        })
    }

    fn chat_with_history(
        &self,
        _system: &str,
        messages: &[ChatMessage],
    ) -> Result<ProviderResponse> {
        let last_msg = messages.last()
            .map(|m| m.content.as_str())
            .unwrap_or("(empty)");
        Ok(ProviderResponse {
            content: format!("[noop-test] {}", last_msg),
            provider_id: "noop-test",
            usage: Some(TokenUsage::default()),
        })
    }
}

/// FailingProvider for testing — always returns an error
pub struct FailingTestProvider;

impl Provider for FailingTestProvider {
    fn id(&self) -> &'static str { "failing-test" }
    fn display_name(&self) -> &str { "Failing Test Provider" }
    fn is_available(&self) -> bool { true }

    fn chat(&self, _system: &str, _user_message: &str) -> Result<ProviderResponse> {
        Err(crate::error::OneClawError::Provider("test provider always fails".into()))
    }

    fn chat_with_history(
        &self,
        _system: &str,
        _messages: &[ChatMessage],
    ) -> Result<ProviderResponse> {
        Err(crate::error::OneClawError::Provider("test provider always fails".into()))
    }
}

/// CountingProvider for testing — fails N times then succeeds
pub struct CountingTestProvider {
    fail_count: std::sync::atomic::AtomicU32,
    fail_until: u32,
}

impl CountingTestProvider {
    /// Create a provider that fails `fail_until` times then succeeds
    pub fn new(fail_until: u32) -> Self {
        Self {
            fail_count: std::sync::atomic::AtomicU32::new(0),
            fail_until,
        }
    }
}

impl Provider for CountingTestProvider {
    fn id(&self) -> &'static str { "counting-test" }
    fn display_name(&self) -> &str { "Counting Test Provider" }
    fn is_available(&self) -> bool { true }

    fn chat(&self, _system: &str, user_message: &str) -> Result<ProviderResponse> {
        let count = self.fail_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if count < self.fail_until {
            Err(crate::error::OneClawError::Provider(format!("fail #{}", count + 1)))
        } else {
            Ok(ProviderResponse {
                content: format!("[counting] {}", user_message),
                provider_id: "counting-test",
                usage: None,
            })
        }
    }

    fn chat_with_history(
        &self,
        system: &str,
        messages: &[ChatMessage],
    ) -> Result<ProviderResponse> {
        let last = messages.last().map(|m| m.content.as_str()).unwrap_or("");
        self.chat(system, last)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_trait_object_safety() {
        // Verify Provider can be used as dyn Provider (object-safe)
        let provider: Box<dyn Provider> = Box::new(NoopTestProvider::available());
        assert_eq!(provider.id(), "noop-test");
        assert!(provider.is_available());
        let resp = provider.chat("system", "hello").unwrap();
        assert!(resp.content.contains("hello"));
    }

    #[test]
    fn test_reliable_provider_retries_then_succeeds() {
        // CountingProvider fails 2 times then succeeds
        let inner = CountingTestProvider::new(2);
        let reliable = ReliableProvider::new(inner, 2);
        let resp = reliable.chat("system", "test").unwrap();
        assert!(resp.content.contains("test"));
    }

    #[test]
    fn test_reliable_provider_exhausts_retries() {
        // Provider always fails, max_retries=2 → 3 attempts total → error
        let inner = CountingTestProvider::new(100); // never succeeds
        let reliable = ReliableProvider::new(inner, 2);
        let result = reliable.chat("system", "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_fallback_chain_primary_succeeds() {
        let chain = FallbackChain::new(vec![
            Box::new(NoopTestProvider::available()),
            Box::new(NoopTestProvider::available()),
        ]);
        let resp = chain.chat("system", "hello").unwrap();
        // First provider should respond
        assert!(resp.content.contains("hello"));
    }

    #[test]
    fn test_fallback_chain_primary_fails() {
        let chain = FallbackChain::new(vec![
            Box::new(FailingTestProvider),
            Box::new(NoopTestProvider::available()),
        ]);
        let resp = chain.chat("system", "fallback").unwrap();
        assert!(resp.content.contains("fallback"));
        assert_eq!(resp.provider_id, "noop-test");
    }

    #[test]
    fn test_fallback_chain_all_fail() {
        let chain = FallbackChain::new(vec![
            Box::new(FailingTestProvider),
            Box::new(FailingTestProvider),
        ]);
        let result = chain.chat("system", "test");
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("All providers"));
    }

    #[test]
    fn test_fallback_chain_skips_unavailable() {
        let chain = FallbackChain::new(vec![
            Box::new(NoopTestProvider::unavailable()),
            Box::new(NoopTestProvider::available()),
        ]);
        let resp = chain.chat("system", "skip").unwrap();
        assert!(resp.content.contains("skip"));
    }

    #[test]
    fn test_chat_with_history() {
        let provider = NoopTestProvider::available();
        let messages = vec![
            ChatMessage { role: MessageRole::User, content: "first".into() },
            ChatMessage { role: MessageRole::User, content: "second".into() },
        ];
        let resp = provider.chat_with_history("system", &messages).unwrap();
        assert!(resp.content.contains("second"));
    }

    #[test]
    fn test_provider_config_struct() {
        let config = ProviderConfig {
            provider_id: "anthropic".into(),
            endpoint: None,
            api_key: Some("sk-test".into()),
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 1024,
            temperature: 0.3,
        };
        assert_eq!(config.provider_id, "anthropic");
        assert_eq!(config.max_tokens, 1024);
    }

    #[test]
    fn test_fallback_chain_is_available() {
        let chain = FallbackChain::new(vec![
            Box::new(NoopTestProvider::unavailable()),
            Box::new(NoopTestProvider::available()),
        ]);
        assert!(chain.is_available());

        let chain_none = FallbackChain::new(vec![
            Box::new(NoopTestProvider::unavailable()),
        ]);
        assert!(!chain_none.is_available());
    }

}
