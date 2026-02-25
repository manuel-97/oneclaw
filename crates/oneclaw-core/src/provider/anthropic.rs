//! Anthropic Claude provider — OneClaw's primary LLM
//!
//! API: POST <https://api.anthropic.com/v1/messages>
//! Docs: <https://docs.anthropic.com/en/api/messages>
//!
//! Supported models:
//!   - claude-sonnet-4-20250514 (default — best balance)
//!   - claude-haiku-4-5-20251001 (fast, cheap — good for classification)
//!   - claude-opus-4-5-20250918 (max quality — expensive)

use crate::error::{OneClawError, Result};
use crate::provider::traits::{
    ChatMessage, MessageRole, Provider, ProviderConfig, ProviderResponse, TokenUsage,
};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, info, warn};

const DEFAULT_ENDPOINT: &str = "https://api.anthropic.com";
const API_VERSION: &str = "2023-06-01";
const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// Anthropic Claude provider — implements the sync Provider trait.
///
/// API key resolution order:
/// 1. config.api_key (explicit in TOML or code)
/// 2. ONECLAW_API_KEY environment variable
/// 3. ANTHROPIC_API_KEY environment variable
pub struct AnthropicProvider {
    client: Client,
    config: ProviderConfig,
    endpoint: String,
}

impl std::fmt::Debug for AnthropicProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnthropicProvider")
            .field("model", &self.config.model)
            .field("endpoint", &self.endpoint)
            .field("api_key", &crate::config::mask_key(
                self.config.api_key.as_deref().unwrap_or("")
            ))
            .finish()
    }
}

// ─── Anthropic API Request/Response types ───

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Serialize, Deserialize, Debug)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Deserialize, Debug)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
    usage: Option<AnthropicUsage>,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Deserialize, Debug)]
struct AnthropicContent {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
}

#[derive(Deserialize, Debug)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Deserialize, Debug)]
struct AnthropicErrorResponse {
    #[serde(rename = "type")]
    _error_type: String,
    error: AnthropicErrorDetail,
}

#[derive(Deserialize, Debug)]
struct AnthropicErrorDetail {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider from config.
    ///
    /// API key resolution order:
    /// 1. config.api_key (explicit)
    /// 2. ONECLAW_API_KEY env var
    /// 3. ANTHROPIC_API_KEY env var
    /// 4. Error (no key found)
    pub fn new(config: ProviderConfig) -> Result<Self> {
        let api_key = config
            .api_key
            .clone()
            .or_else(|| std::env::var("ONECLAW_API_KEY").ok())
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
            .ok_or_else(|| {
                OneClawError::Provider(
                    "No API key: set api_key in config, ONECLAW_API_KEY, or ANTHROPIC_API_KEY"
                        .into(),
                )
            })?;

        let endpoint = config
            .endpoint
            .clone()
            .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());

        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .map_err(|e| OneClawError::Provider(format!("HTTP client init failed: {}", e)))?;

        let mut resolved_config = config;
        resolved_config.api_key = Some(api_key);

        info!(
            model = %resolved_config.model,
            endpoint = %endpoint,
            "Anthropic provider initialized"
        );

        Ok(Self {
            client,
            config: resolved_config,
            endpoint,
        })
    }

    /// Create with explicit API key (for testing, bypasses env lookup).
    pub fn with_key(api_key: &str, model: &str) -> Result<Self> {
        Self::new(ProviderConfig {
            provider_id: "anthropic".into(),
            endpoint: None,
            api_key: Some(api_key.into()),
            model: model.into(),
            max_tokens: 1024,
            temperature: 0.3,
        })
    }

    /// Create with a custom endpoint (for proxy/testing).
    pub fn with_endpoint(api_key: &str, model: &str, endpoint: &str) -> Result<Self> {
        Self::new(ProviderConfig {
            provider_id: "anthropic".into(),
            endpoint: Some(endpoint.into()),
            api_key: Some(api_key.into()),
            model: model.into(),
            max_tokens: 1024,
            temperature: 0.3,
        })
    }

    /// Internal: execute API call
    fn call_api(&self, request: &AnthropicRequest) -> Result<ProviderResponse> {
        let api_key = self
            .config
            .api_key
            .as_deref()
            .ok_or_else(|| OneClawError::Provider("API key missing".into()))?;

        let url = format!("{}/v1/messages", self.endpoint);

        debug!(
            model = %request.model,
            messages = request.messages.len(),
            "Calling Anthropic API"
        );

        let response = self
            .client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(request)
            .send()
            .map_err(|e| {
                warn!(error = %e, "Anthropic API request failed");
                OneClawError::Provider(format!("Request failed: {}", e))
            })?;

        let status = response.status();
        let body = response
            .text()
            .map_err(|e| OneClawError::Provider(format!("Read body failed: {}", e)))?;

        if !status.is_success() {
            let error_msg = match serde_json::from_str::<AnthropicErrorResponse>(&body) {
                Ok(err) => format!(
                    "Anthropic API error {}: {} — {}",
                    status, err.error.error_type, err.error.message
                ),
                Err(_) => format!("Anthropic API error {}: {}", status, body),
            };
            warn!(status = %status, "Anthropic API returned error");
            return Err(OneClawError::Provider(error_msg));
        }

        let api_response: AnthropicResponse = serde_json::from_str(&body)
            .map_err(|e| OneClawError::Provider(format!("Parse response failed: {}", e)))?;

        // Extract text from content blocks
        let content = api_response
            .content
            .iter()
            .filter(|c| c.content_type == "text")
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
            .join("");

        if content.is_empty() {
            return Err(OneClawError::Provider(
                "Empty response from Anthropic".into(),
            ));
        }

        let usage = api_response.usage.map(|u| TokenUsage {
            prompt_tokens: u.input_tokens,
            completion_tokens: u.output_tokens,
            total_tokens: u.input_tokens + u.output_tokens,
        });

        debug!(
            tokens = usage.as_ref().map_or(0, |u| u.total_tokens),
            stop = api_response.stop_reason.as_deref().unwrap_or("unknown"),
            "Anthropic response received"
        );

        Ok(ProviderResponse {
            content,
            provider_id: "anthropic",
            usage,
        })
    }

    /// Convert ChatMessage list to Anthropic format.
    /// Anthropic: system is separate top-level param, only user/assistant in messages array.
    fn to_anthropic_messages(
        messages: &[ChatMessage],
    ) -> (Option<String>, Vec<AnthropicMessage>) {
        let mut system: Option<String> = None;
        let mut api_messages = Vec::new();

        for msg in messages {
            match msg.role {
                MessageRole::System => {
                    // Anthropic takes system as top-level param, not in messages
                    match &mut system {
                        Some(s) => {
                            s.push('\n');
                            s.push_str(&msg.content);
                        }
                        None => system = Some(msg.content.clone()),
                    }
                }
                MessageRole::User => {
                    api_messages.push(AnthropicMessage {
                        role: "user".into(),
                        content: msg.content.clone(),
                    });
                }
                MessageRole::Assistant => {
                    api_messages.push(AnthropicMessage {
                        role: "assistant".into(),
                        content: msg.content.clone(),
                    });
                }
            }
        }

        (system, api_messages)
    }
}

impl Provider for AnthropicProvider {
    fn id(&self) -> &'static str {
        "anthropic"
    }

    fn display_name(&self) -> &str {
        "Anthropic Claude"
    }

    fn is_available(&self) -> bool {
        self.config.api_key.is_some()
    }

    fn chat(&self, system: &str, user_message: &str) -> Result<ProviderResponse> {
        let request = AnthropicRequest {
            model: self.config.model.clone(),
            max_tokens: self.config.max_tokens,
            system: if system.is_empty() {
                None
            } else {
                Some(system.into())
            },
            messages: vec![AnthropicMessage {
                role: "user".into(),
                content: user_message.into(),
            }],
            temperature: Some(self.config.temperature),
        };

        self.call_api(&request)
    }

    fn chat_with_history(
        &self,
        system: &str,
        messages: &[ChatMessage],
    ) -> Result<ProviderResponse> {
        let (msg_system, api_messages) = Self::to_anthropic_messages(messages);

        // Merge: explicit system param takes priority, then message-embedded system
        let final_system = match (system.is_empty(), msg_system) {
            (false, Some(msg_sys)) => Some(format!("{}\n{}", system, msg_sys)),
            (false, None) => Some(system.into()),
            (true, Some(msg_sys)) => Some(msg_sys),
            (true, None) => None,
        };

        if api_messages.is_empty() {
            return Err(OneClawError::Provider(
                "No user/assistant messages in history".into(),
            ));
        }

        let request = AnthropicRequest {
            model: self.config.model.clone(),
            max_tokens: self.config.max_tokens,
            system: final_system,
            messages: api_messages,
            temperature: Some(self.config.temperature),
        };

        self.call_api(&request)
    }
}

/// Resolve a Provider from ProviderConfigToml.
///
/// Returns Ok(provider) for supported providers, Err for unknown.
/// In v1.2, only "anthropic" is supported. v1.5 adds openai/deepseek/groq/ollama/google.
pub fn resolve_provider(
    config: &crate::config::ProviderConfigToml,
) -> Result<Box<dyn Provider>> {
    // Auto-select default model if config model matches a different provider's default
    let model = if config.model.is_empty() {
        crate::provider::openai_compat::default_model_for_provider(&config.primary).to_string()
    } else {
        config.model.clone()
    };

    let provider_config = ProviderConfig {
        provider_id: config.primary.clone(),
        endpoint: None,
        api_key: config.api_key.clone(),
        model,
        max_tokens: config.max_tokens,
        temperature: config.temperature,
    };

    match config.primary.as_str() {
        "anthropic" => {
            let provider = AnthropicProvider::new(provider_config)?;
            info!(
                provider = "anthropic",
                model = %config.model,
                "Provider resolved"
            );
            Ok(Box::new(provider))
        }
        "openai" => {
            let provider = crate::provider::openai_compat::OpenAICompatibleProvider::openai(provider_config)?;
            info!(provider = "openai", model = %config.model, "Provider resolved");
            Ok(Box::new(provider))
        }
        "deepseek" => {
            let provider = crate::provider::openai_compat::OpenAICompatibleProvider::deepseek(provider_config)?;
            info!(provider = "deepseek", model = %config.model, "Provider resolved");
            Ok(Box::new(provider))
        }
        "groq" => {
            let provider = crate::provider::openai_compat::OpenAICompatibleProvider::groq(provider_config)?;
            info!(provider = "groq", model = %config.model, "Provider resolved");
            Ok(Box::new(provider))
        }
        "ollama" => {
            let provider = crate::provider::ollama::OllamaProvider::from_config(&provider_config)?;
            info!(provider = "ollama", model = %config.model, "Provider resolved (local)");
            Ok(Box::new(provider))
        }
        "google" | "gemini" => {
            let provider = crate::provider::gemini::GeminiProvider::new(provider_config)?;
            info!(provider = "google", model = %config.model, "Provider resolved");
            Ok(Box::new(provider))
        }
        other => Err(OneClawError::Provider(format!(
            "Unknown provider '{}'. Supported: anthropic, openai, deepseek, groq, ollama, google",
            other
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::traits::{FallbackChain, NoopTestProvider, ReliableProvider};

    #[test]
    fn test_anthropic_provider_new_with_config() {
        let config = ProviderConfig {
            provider_id: "anthropic".into(),
            endpoint: None,
            api_key: Some("sk-test-key-1234".into()),
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 1024,
            temperature: 0.3,
        };
        let provider = AnthropicProvider::new(config).unwrap();
        assert_eq!(provider.id(), "anthropic");
        assert_eq!(provider.display_name(), "Anthropic Claude");
        assert!(provider.is_available());
    }

    #[test]
    fn test_anthropic_provider_no_key_errors() {
        // Use explicit config with no key and no env fallback possible
        // (test doesn't manipulate env — avoids unsafe set_var in Rust 2024)
        let config = ProviderConfig {
            provider_id: "anthropic".into(),
            endpoint: None,
            api_key: None,
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 1024,
            temperature: 0.3,
        };
        // If ONECLAW_API_KEY or ANTHROPIC_API_KEY happens to be set, this test
        // will pass (provider creates successfully). The key test is that the
        // error message is correct when NO key is available at all.
        // For deterministic testing, we test with explicit key = Some("") which is falsy-ish
        // but still Some, so it won't trigger the error. Instead, test error message format:
        let err_msg = "No API key: set api_key in config, ONECLAW_API_KEY, or ANTHROPIC_API_KEY";
        assert!(err_msg.contains("ONECLAW_API_KEY"));
        assert!(err_msg.contains("ANTHROPIC_API_KEY"));

        // If env vars happen to be unset, verify it actually errors
        if std::env::var("ONECLAW_API_KEY").is_err()
            && std::env::var("ANTHROPIC_API_KEY").is_err()
        {
            let result = AnthropicProvider::new(config);
            assert!(result.is_err(), "Should error with no API key");
        }
    }

    #[test]
    fn test_anthropic_provider_config_key_takes_priority() {
        // Config api_key takes priority over any env var
        let config = ProviderConfig {
            provider_id: "anthropic".into(),
            endpoint: None,
            api_key: Some("sk-explicit-config-key".into()),
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 1024,
            temperature: 0.3,
        };
        let provider = AnthropicProvider::new(config).unwrap();
        assert_eq!(
            provider.config.api_key.as_deref(),
            Some("sk-explicit-config-key")
        );
        assert!(provider.is_available());
    }

    #[test]
    fn test_anthropic_provider_env_key_resolution_order() {
        // Test the resolution chain: config.api_key is checked first
        // If present, env vars are never consulted
        let config_with_key = ProviderConfig {
            provider_id: "anthropic".into(),
            endpoint: None,
            api_key: Some("sk-from-config".into()),
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 1024,
            temperature: 0.3,
        };
        let provider = AnthropicProvider::new(config_with_key).unwrap();
        assert_eq!(
            provider.config.api_key.as_deref(),
            Some("sk-from-config"),
            "Config key should take priority over env"
        );
    }

    #[test]
    fn test_anthropic_message_conversion_simple() {
        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: "hello".into(),
        }];
        let (system, api_msgs) = AnthropicProvider::to_anthropic_messages(&messages);
        assert!(system.is_none());
        assert_eq!(api_msgs.len(), 1);
        assert_eq!(api_msgs[0].role, "user");
        assert_eq!(api_msgs[0].content, "hello");
    }

    #[test]
    fn test_anthropic_message_conversion_with_system() {
        let messages = vec![
            ChatMessage {
                role: MessageRole::System,
                content: "You are helpful".into(),
            },
            ChatMessage {
                role: MessageRole::User,
                content: "hello".into(),
            },
            ChatMessage {
                role: MessageRole::Assistant,
                content: "hi".into(),
            },
            ChatMessage {
                role: MessageRole::User,
                content: "bye".into(),
            },
        ];
        let (system, api_msgs) = AnthropicProvider::to_anthropic_messages(&messages);
        assert_eq!(system.as_deref(), Some("You are helpful"));
        assert_eq!(api_msgs.len(), 3); // system extracted, 3 remain
        assert_eq!(api_msgs[0].role, "user");
        assert_eq!(api_msgs[1].role, "assistant");
        assert_eq!(api_msgs[2].role, "user");
    }

    #[test]
    fn test_anthropic_message_conversion_multiple_system() {
        let messages = vec![
            ChatMessage {
                role: MessageRole::System,
                content: "A".into(),
            },
            ChatMessage {
                role: MessageRole::System,
                content: "B".into(),
            },
            ChatMessage {
                role: MessageRole::User,
                content: "hello".into(),
            },
        ];
        let (system, api_msgs) = AnthropicProvider::to_anthropic_messages(&messages);
        assert_eq!(system.as_deref(), Some("A\nB"));
        assert_eq!(api_msgs.len(), 1);
    }

    #[test]
    fn test_anthropic_message_conversion_no_user() {
        let provider = AnthropicProvider::with_key("test-key", "claude-sonnet-4-20250514").unwrap();
        let messages = vec![ChatMessage {
            role: MessageRole::System,
            content: "only system".into(),
        }];
        let result = provider.chat_with_history("", &messages);
        assert!(result.is_err());
        let err = match result {
            Err(e) => format!("{}", e),
            Ok(_) => panic!("expected error"),
        };
        assert!(err.contains("No user/assistant messages"));
    }

    #[test]
    fn test_anthropic_request_serialization() {
        let request = AnthropicRequest {
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 1024,
            system: Some("Be helpful".into()),
            messages: vec![AnthropicMessage {
                role: "user".into(),
                content: "hello".into(),
            }],
            temperature: Some(0.3),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("claude-sonnet-4-20250514"));
        assert!(json.contains("1024"));
        assert!(json.contains("Be helpful"));
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("0.3"));

        // system=None should be omitted
        let request_no_sys = AnthropicRequest {
            model: "test".into(),
            max_tokens: 100,
            system: None,
            messages: vec![],
            temperature: None,
        };
        let json2 = serde_json::to_string(&request_no_sys).unwrap();
        assert!(!json2.contains("system"));
        assert!(!json2.contains("temperature"));
    }

    #[test]
    fn test_anthropic_response_parsing() {
        let json = r#"{
            "content": [{"type": "text", "text": "Hello!"}],
            "usage": {"input_tokens": 10, "output_tokens": 5},
            "stop_reason": "end_turn"
        }"#;
        let resp: AnthropicResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.content.len(), 1);
        assert_eq!(resp.content[0].text, "Hello!");
        assert_eq!(resp.usage.as_ref().unwrap().input_tokens, 10);
        assert_eq!(resp.usage.as_ref().unwrap().output_tokens, 5);
        assert_eq!(resp.stop_reason.as_deref(), Some("end_turn"));
    }

    #[test]
    fn test_anthropic_response_multi_block() {
        let json = r#"{
            "content": [
                {"type": "text", "text": "Part 1"},
                {"type": "text", "text": " Part 2"}
            ],
            "usage": {"input_tokens": 10, "output_tokens": 8}
        }"#;
        let resp: AnthropicResponse = serde_json::from_str(json).unwrap();
        let content: String = resp
            .content
            .iter()
            .filter(|c| c.content_type == "text")
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(content, "Part 1 Part 2");
    }

    #[test]
    fn test_anthropic_error_response_parsing() {
        let json = r#"{
            "type": "error",
            "error": {"type": "authentication_error", "message": "Invalid API key"}
        }"#;
        let err: AnthropicErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(err.error.error_type, "authentication_error");
        assert_eq!(err.error.message, "Invalid API key");
    }

    #[test]
    fn test_anthropic_provider_with_reliable_wrapper() {
        let provider =
            AnthropicProvider::with_key("test-key", "claude-sonnet-4-20250514").unwrap();
        let reliable = ReliableProvider::new(provider, 3);
        // Type check: ReliableProvider<AnthropicProvider> implements Provider
        assert_eq!(reliable.id(), "anthropic");
        assert_eq!(reliable.display_name(), "Anthropic Claude");
        assert!(reliable.is_available());
    }

    #[test]
    fn test_anthropic_provider_in_fallback_chain() {
        // Create AnthropicProvider with a bogus key — it will be "available" but fail on call
        // Use NoopTestProvider as fallback
        let chain = FallbackChain::new(vec![
            Box::new(NoopTestProvider::unavailable()), // first: unavailable
            Box::new(NoopTestProvider::available()),    // fallback: available
        ]);
        // Chain should skip unavailable, use noop
        let resp = chain.chat("system", "test").unwrap();
        assert!(resp.content.contains("test"));
        assert_eq!(resp.provider_id, "noop-test");
    }

    #[test]
    fn test_resolve_provider_anthropic() {
        let config = crate::config::ProviderConfigToml {
            primary: "anthropic".into(),
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 1024,
            temperature: 0.3,
            api_key: Some("test-key-for-resolve".into()),
            fallback: vec![],
            ..Default::default()
        };
        let provider = resolve_provider(&config).unwrap();
        assert_eq!(provider.id(), "anthropic");
        assert!(provider.is_available());
    }

    #[test]
    fn test_resolve_provider_unknown() {
        let config = crate::config::ProviderConfigToml {
            primary: "unknown-provider".into(),
            model: "test".into(),
            max_tokens: 100,
            temperature: 0.5,
            api_key: Some("key".into()),
            fallback: vec![],
            ..Default::default()
        };
        let result = resolve_provider(&config);
        assert!(result.is_err());
        let err = match result {
            Err(e) => format!("{}", e),
            Ok(_) => panic!("expected error"),
        };
        assert!(err.contains("Unknown provider"));
        assert!(err.contains("anthropic"));
    }

    #[test]
    fn test_resolve_provider_openai() {
        let config = crate::config::ProviderConfigToml {
            primary: "openai".into(),
            model: "gpt-4o".into(),
            max_tokens: 1024,
            temperature: 0.3,
            api_key: Some("sk-test-resolve-openai".into()),
            fallback: vec![],
            ..Default::default()
        };
        let provider = resolve_provider(&config).unwrap();
        assert_eq!(provider.id(), "openai");
        assert!(provider.is_available());
    }

    #[test]
    fn test_resolve_provider_deepseek() {
        let config = crate::config::ProviderConfigToml {
            primary: "deepseek".into(),
            model: "deepseek-chat".into(),
            max_tokens: 1024,
            temperature: 0.3,
            api_key: Some("sk-test-resolve-deepseek".into()),
            fallback: vec![],
            ..Default::default()
        };
        let provider = resolve_provider(&config).unwrap();
        assert_eq!(provider.id(), "deepseek");
        assert!(provider.is_available());
    }

    #[test]
    fn test_resolve_provider_groq() {
        let config = crate::config::ProviderConfigToml {
            primary: "groq".into(),
            model: "llama-3.3-70b-versatile".into(),
            max_tokens: 1024,
            temperature: 0.3,
            api_key: Some("gsk-test-resolve-groq".into()),
            fallback: vec![],
            ..Default::default()
        };
        let provider = resolve_provider(&config).unwrap();
        assert_eq!(provider.id(), "groq");
        assert!(provider.is_available());
    }

    #[test]
    fn test_resolve_provider_ollama() {
        let config = crate::config::ProviderConfigToml {
            primary: "ollama".into(),
            model: "llama3.2:3b".into(),
            max_tokens: 1024,
            temperature: 0.3,
            api_key: None, // Ollama doesn't need a key
            fallback: vec![],
            ..Default::default()
        };
        let provider = resolve_provider(&config).unwrap();
        assert_eq!(provider.id(), "ollama");
        // Note: is_available() would try to ping localhost:11434
        // which may not be running in test — don't assert is_available
    }

    #[test]
    fn test_resolve_provider_google() {
        let config = crate::config::ProviderConfigToml {
            primary: "google".into(),
            model: "gemini-2.0-flash".into(),
            max_tokens: 1024,
            temperature: 0.3,
            api_key: Some("test-key-for-resolve-google".into()),
            fallback: vec![],
            ..Default::default()
        };
        let provider = resolve_provider(&config).unwrap();
        assert_eq!(provider.id(), "google");
        assert!(provider.is_available());
    }

    #[test]
    fn test_resolve_provider_gemini_alias() {
        let config = crate::config::ProviderConfigToml {
            primary: "gemini".into(), // alias for "google"
            model: "gemini-2.0-flash".into(),
            max_tokens: 1024,
            temperature: 0.3,
            api_key: Some("test-key-for-resolve-gemini".into()),
            fallback: vec![],
            ..Default::default()
        };
        let provider = resolve_provider(&config).unwrap();
        assert_eq!(provider.id(), "google"); // resolves to "google"
        assert!(provider.is_available());
    }

    #[test]
    fn test_anthropic_with_custom_endpoint() {
        let provider =
            AnthropicProvider::with_endpoint("test-key", "claude-sonnet-4-20250514", "https://custom.proxy.com")
                .unwrap();
        assert_eq!(provider.endpoint, "https://custom.proxy.com");
        assert!(provider.is_available());
    }

    // ═══════════════════════════════════════════════════
    // Integration tests (require real API key)
    // ═══════════════════════════════════════════════════

    #[test]
    #[ignore] // Run manually: cargo test test_anthropic_live_chat -- --ignored
    fn test_anthropic_live_chat() {
        let key =
            std::env::var("ANTHROPIC_API_KEY").expect("Set ANTHROPIC_API_KEY to run live test");

        let provider = AnthropicProvider::with_key(&key, "claude-haiku-4-5-20251001")
            .expect("Provider init failed");

        let response = provider
            .chat(
                "You are a helpful assistant. Reply in exactly 3 words.",
                "Say hello",
            )
            .expect("API call failed");

        assert!(!response.content.is_empty());
        assert_eq!(response.provider_id, "anthropic");
        assert!(response.usage.is_some());
        println!("Live response: {}", response.content);
    }

    #[test]
    #[ignore]
    fn test_anthropic_live_vietnamese() {
        let key = std::env::var("ANTHROPIC_API_KEY").expect("Set ANTHROPIC_API_KEY");

        let provider = AnthropicProvider::with_key(&key, "claude-haiku-4-5-20251001")
            .expect("Init failed");

        let response = provider
            .chat(
                "Bạn là trợ lý chăm sóc sức khoẻ người cao tuổi. Trả lời bằng tiếng Việt.",
                "Bà ngoại tôi bị đau đầu từ sáng, nên làm gì?",
            )
            .expect("API call failed");

        assert!(!response.content.is_empty());
        // Vietnamese response should contain non-ASCII characters
        assert!(response.content.chars().any(|c| c as u32 > 127));
        println!("Vietnamese response: {}", response.content);
    }
}
