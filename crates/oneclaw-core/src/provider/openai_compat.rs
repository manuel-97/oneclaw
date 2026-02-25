//! OpenAI-Compatible provider — 1 struct, 3 providers
//!
//! "3 con dao, 1 lần mài" — one implementation serves:
//!   - OpenAI (GPT-4o, GPT-4o-mini)
//!   - DeepSeek (deepseek-chat, deepseek-reasoner)
//!   - Groq (llama-3.3-70b-versatile, mixtral-8x7b-32768)
//!
//! All share the OpenAI Chat Completions API format:
//!   POST {endpoint}/v1/chat/completions
//!   Authorization: Bearer {api_key}
//!   Messages: [{ role: "system"|"user"|"assistant", content: "..." }]

use crate::error::{OneClawError, Result};
use crate::provider::traits::{
    ChatMessage, MessageRole, Provider, ProviderConfig, ProviderResponse, TokenUsage,
};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, info, warn};

const DEFAULT_TIMEOUT_SECS: u64 = 60;

// ─── Provider Presets ───────────────────────────────────────

/// Preset configuration for a known provider.
#[derive(Debug, Clone)]
pub struct ProviderPreset {
    /// Provider ID used in config routing
    pub id: &'static str,
    /// Human-readable display name
    pub display_name: &'static str,
    /// Default API endpoint
    pub endpoint: &'static str,
    /// Default model name
    pub default_model: &'static str,
    /// Environment variable for API key (provider-specific)
    pub env_key: &'static str,
}

/// OpenAI preset — GPT-4o family
pub const PRESET_OPENAI: ProviderPreset = ProviderPreset {
    id: "openai",
    display_name: "OpenAI GPT",
    endpoint: "https://api.openai.com",
    default_model: "gpt-4o",
    env_key: "OPENAI_API_KEY",
};

/// DeepSeek preset — deepseek-chat family
pub const PRESET_DEEPSEEK: ProviderPreset = ProviderPreset {
    id: "deepseek",
    display_name: "DeepSeek",
    endpoint: "https://api.deepseek.com",
    default_model: "deepseek-chat",
    env_key: "DEEPSEEK_API_KEY",
};

/// Groq preset — fast inference (Llama, Mixtral)
pub const PRESET_GROQ: ProviderPreset = ProviderPreset {
    id: "groq",
    display_name: "Groq",
    endpoint: "https://api.groq.com/openai",
    default_model: "llama-3.3-70b-versatile",
    env_key: "GROQ_API_KEY",
};

// ─── OpenAI Chat Completions API types ──────────────────────

#[derive(Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct OpenAIMessage {
    role: String,
    content: String,
}

#[derive(Deserialize, Debug)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Deserialize, Debug)]
struct OpenAIChoice {
    message: OpenAIMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Debug)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Deserialize, Debug)]
struct OpenAIErrorResponse {
    error: OpenAIErrorDetail,
}

#[derive(Deserialize, Debug)]
struct OpenAIErrorDetail {
    message: String,
    #[serde(default)]
    #[serde(rename = "type")]
    error_type: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    code: Option<String>,
}

// ─── OpenAICompatibleProvider ───────────────────────────────

/// A single provider struct that serves OpenAI, DeepSeek, and Groq.
///
/// All three use the OpenAI Chat Completions API format:
/// - System prompt is a message with role "system" (not top-level param)
/// - Authorization via `Bearer` token
/// - POST to `{endpoint}/v1/chat/completions`
///
/// API key resolution order:
/// 1. config.api_key (explicit in TOML or code)
/// 2. ONECLAW_API_KEY environment variable
/// 3. Provider-specific env var (OPENAI_API_KEY, DEEPSEEK_API_KEY, GROQ_API_KEY)
pub struct OpenAICompatibleProvider {
    client: Client,
    config: ProviderConfig,
    preset: ProviderPreset,
    endpoint: String,
}

impl std::fmt::Debug for OpenAICompatibleProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAICompatibleProvider")
            .field("provider", &self.preset.id)
            .field("model", &self.config.model)
            .field("endpoint", &self.endpoint)
            .field("api_key", &crate::config::mask_key(
                self.config.api_key.as_deref().unwrap_or("")
            ))
            .finish()
    }
}

impl OpenAICompatibleProvider {
    /// Create from a preset and ProviderConfig.
    ///
    /// API key resolution: config.api_key → ONECLAW_API_KEY → preset.env_key
    pub fn new(preset: ProviderPreset, config: ProviderConfig) -> Result<Self> {
        let api_key = config
            .api_key
            .clone()
            .or_else(|| std::env::var("ONECLAW_API_KEY").ok())
            .or_else(|| std::env::var(preset.env_key).ok())
            .ok_or_else(|| {
                OneClawError::Provider(format!(
                    "No API key for {}: set api_key in config, ONECLAW_API_KEY, or {}",
                    preset.id, preset.env_key
                ))
            })?;

        let endpoint = config
            .endpoint
            .clone()
            .unwrap_or_else(|| preset.endpoint.to_string());

        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .map_err(|e| OneClawError::Provider(format!("HTTP client init failed: {}", e)))?;

        let mut resolved_config = config;
        resolved_config.api_key = Some(api_key);

        info!(
            provider = preset.id,
            model = %resolved_config.model,
            endpoint = %endpoint,
            "OpenAI-compatible provider initialized"
        );

        Ok(Self {
            client,
            config: resolved_config,
            preset,
            endpoint,
        })
    }

    // ─── Convenience constructors ───────────────────────────

    /// Create an OpenAI provider (GPT-4o).
    pub fn openai(config: ProviderConfig) -> Result<Self> {
        Self::new(PRESET_OPENAI, config)
    }

    /// Create a DeepSeek provider (deepseek-chat).
    pub fn deepseek(config: ProviderConfig) -> Result<Self> {
        Self::new(PRESET_DEEPSEEK, config)
    }

    /// Create a Groq provider (llama-3.3-70b-versatile).
    pub fn groq(config: ProviderConfig) -> Result<Self> {
        Self::new(PRESET_GROQ, config)
    }

    /// Create a custom OpenAI-compatible provider with arbitrary endpoint.
    pub fn custom(
        id: &'static str,
        display_name: &'static str,
        endpoint: &str,
        env_key: &'static str,
        default_model: &'static str,
        config: ProviderConfig,
    ) -> Result<Self> {
        let preset = ProviderPreset {
            id,
            display_name,
            endpoint: "", // overridden by config.endpoint below
            default_model,
            env_key,
        };
        let mut cfg = config;
        cfg.endpoint = Some(endpoint.to_string());
        Self::new(preset, cfg)
    }

    /// Create with explicit API key (for testing, bypasses env lookup).
    pub fn with_key(preset: ProviderPreset, api_key: &str, model: &str) -> Result<Self> {
        Self::new(
            preset,
            ProviderConfig {
                provider_id: "openai-compat".into(),
                endpoint: None,
                api_key: Some(api_key.into()),
                model: model.into(),
                max_tokens: 1024,
                temperature: 0.3,
            },
        )
    }

    // ─── Internal API call ──────────────────────────────────

    fn call_api(&self, request: &OpenAIRequest) -> Result<ProviderResponse> {
        let api_key = self
            .config
            .api_key
            .as_deref()
            .ok_or_else(|| OneClawError::Provider("API key missing".into()))?;

        let url = format!("{}/v1/chat/completions", self.endpoint);

        debug!(
            provider = self.preset.id,
            model = %request.model,
            messages = request.messages.len(),
            "Calling OpenAI-compatible API"
        );

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("content-type", "application/json")
            .json(request)
            .send()
            .map_err(|e| {
                warn!(provider = self.preset.id, error = %e, "API request failed");
                OneClawError::Provider(format!("{} request failed: {}", self.preset.id, e))
            })?;

        let status = response.status();
        let body = response
            .text()
            .map_err(|e| OneClawError::Provider(format!("Read body failed: {}", e)))?;

        if !status.is_success() {
            let error_msg = match serde_json::from_str::<OpenAIErrorResponse>(&body) {
                Ok(err) => format!(
                    "{} API error {}: {} ({})",
                    self.preset.display_name,
                    status,
                    err.error.message,
                    err.error.error_type.as_deref().unwrap_or("unknown"),
                ),
                Err(_) => format!("{} API error {}: {}", self.preset.display_name, status, body),
            };
            warn!(provider = self.preset.id, status = %status, "API returned error");
            return Err(OneClawError::Provider(error_msg));
        }

        let api_response: OpenAIResponse = serde_json::from_str(&body)
            .map_err(|e| OneClawError::Provider(format!("Parse response failed: {}", e)))?;

        let content = api_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        if content.is_empty() {
            return Err(OneClawError::Provider(format!(
                "Empty response from {}",
                self.preset.display_name
            )));
        }

        let usage = api_response.usage.map(|u| TokenUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });

        debug!(
            provider = self.preset.id,
            tokens = usage.as_ref().map_or(0, |u| u.total_tokens),
            finish = api_response.choices.first()
                .and_then(|c| c.finish_reason.as_deref())
                .unwrap_or("unknown"),
            "Response received"
        );

        Ok(ProviderResponse {
            content,
            provider_id: self.provider_id_static(),
            usage,
        })
    }

    /// Convert ChatMessage list to OpenAI format.
    /// OpenAI-compatible: system IS a message role in the messages array.
    fn to_openai_messages(messages: &[ChatMessage]) -> Vec<OpenAIMessage> {
        messages
            .iter()
            .map(|msg| OpenAIMessage {
                role: match msg.role {
                    MessageRole::System => "system".into(),
                    MessageRole::User => "user".into(),
                    MessageRole::Assistant => "assistant".into(),
                },
                content: msg.content.clone(),
            })
            .collect()
    }

    /// Return a &'static str for the provider ID.
    /// Needed because Provider::id() returns &'static str.
    fn provider_id_static(&self) -> &'static str {
        match self.preset.id {
            "openai" => "openai",
            "deepseek" => "deepseek",
            "groq" => "groq",
            _ => "openai-compat",
        }
    }
}

impl Provider for OpenAICompatibleProvider {
    fn id(&self) -> &'static str {
        self.provider_id_static()
    }

    fn display_name(&self) -> &str {
        self.preset.display_name
    }

    fn is_available(&self) -> bool {
        self.config.api_key.is_some()
    }

    fn chat(&self, system: &str, user_message: &str) -> Result<ProviderResponse> {
        let mut messages = Vec::new();

        if !system.is_empty() {
            messages.push(OpenAIMessage {
                role: "system".into(),
                content: system.into(),
            });
        }

        messages.push(OpenAIMessage {
            role: "user".into(),
            content: user_message.into(),
        });

        let request = OpenAIRequest {
            model: self.config.model.clone(),
            messages,
            max_tokens: self.config.max_tokens,
            temperature: Some(self.config.temperature),
        };

        self.call_api(&request)
    }

    fn chat_with_history(
        &self,
        system: &str,
        messages: &[ChatMessage],
    ) -> Result<ProviderResponse> {
        let mut api_messages = Vec::new();

        // Prepend explicit system prompt if provided
        if !system.is_empty() {
            api_messages.push(OpenAIMessage {
                role: "system".into(),
                content: system.into(),
            });
        }

        // Convert all messages (system in messages array stays as-is for OpenAI)
        api_messages.extend(Self::to_openai_messages(messages));

        // Must have at least one user/assistant message
        let has_content = api_messages.iter().any(|m| m.role == "user" || m.role == "assistant");
        if !has_content {
            return Err(OneClawError::Provider(
                "No user/assistant messages in history".into(),
            ));
        }

        let request = OpenAIRequest {
            model: self.config.model.clone(),
            messages: api_messages,
            max_tokens: self.config.max_tokens,
            temperature: Some(self.config.temperature),
        };

        self.call_api(&request)
    }
}

// ─── Helper: default model for provider ─────────────────────

/// Return the default model for a given provider ID.
/// Used when config specifies a provider but not a model.
pub fn default_model_for_provider(provider_id: &str) -> &'static str {
    match provider_id {
        "anthropic" => "claude-sonnet-4-20250514",
        "openai" => PRESET_OPENAI.default_model,
        "deepseek" => PRESET_DEEPSEEK.default_model,
        "groq" => PRESET_GROQ.default_model,
        "ollama" => "llama3.2:1b",
        "google" | "gemini" => "gemini-2.0-flash",
        _ => "gpt-4o", // safe fallback for unknown OpenAI-compat
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::traits::{FallbackChain, NoopTestProvider, ReliableProvider};

    // ─── Preset tests ───────────────────────────────────────

    #[test]
    fn test_preset_openai_values() {
        assert_eq!(PRESET_OPENAI.id, "openai");
        assert_eq!(PRESET_OPENAI.display_name, "OpenAI GPT");
        assert_eq!(PRESET_OPENAI.endpoint, "https://api.openai.com");
        assert_eq!(PRESET_OPENAI.default_model, "gpt-4o");
        assert_eq!(PRESET_OPENAI.env_key, "OPENAI_API_KEY");
    }

    #[test]
    fn test_preset_deepseek_values() {
        assert_eq!(PRESET_DEEPSEEK.id, "deepseek");
        assert_eq!(PRESET_DEEPSEEK.display_name, "DeepSeek");
        assert_eq!(PRESET_DEEPSEEK.endpoint, "https://api.deepseek.com");
        assert_eq!(PRESET_DEEPSEEK.default_model, "deepseek-chat");
        assert_eq!(PRESET_DEEPSEEK.env_key, "DEEPSEEK_API_KEY");
    }

    #[test]
    fn test_preset_groq_values() {
        assert_eq!(PRESET_GROQ.id, "groq");
        assert_eq!(PRESET_GROQ.display_name, "Groq");
        assert_eq!(PRESET_GROQ.endpoint, "https://api.groq.com/openai");
        assert_eq!(PRESET_GROQ.default_model, "llama-3.3-70b-versatile");
        assert_eq!(PRESET_GROQ.env_key, "GROQ_API_KEY");
    }

    // ─── Constructor tests ──────────────────────────────────

    #[test]
    fn test_openai_provider_new() {
        let provider = OpenAICompatibleProvider::with_key(
            PRESET_OPENAI, "sk-test-key-1234", "gpt-4o"
        ).unwrap();
        assert_eq!(provider.id(), "openai");
        assert_eq!(provider.display_name(), "OpenAI GPT");
        assert!(provider.is_available());
        assert_eq!(provider.endpoint, "https://api.openai.com");
    }

    #[test]
    fn test_deepseek_provider_new() {
        let provider = OpenAICompatibleProvider::with_key(
            PRESET_DEEPSEEK, "sk-test-key-5678", "deepseek-chat"
        ).unwrap();
        assert_eq!(provider.id(), "deepseek");
        assert_eq!(provider.display_name(), "DeepSeek");
        assert!(provider.is_available());
        assert_eq!(provider.endpoint, "https://api.deepseek.com");
    }

    #[test]
    fn test_groq_provider_new() {
        let provider = OpenAICompatibleProvider::with_key(
            PRESET_GROQ, "gsk-test-key-9012", "llama-3.3-70b-versatile"
        ).unwrap();
        assert_eq!(provider.id(), "groq");
        assert_eq!(provider.display_name(), "Groq");
        assert!(provider.is_available());
        assert_eq!(provider.endpoint, "https://api.groq.com/openai");
    }

    #[test]
    fn test_convenience_constructor_openai() {
        let config = ProviderConfig {
            provider_id: "openai".into(),
            endpoint: None,
            api_key: Some("sk-test".into()),
            model: "gpt-4o-mini".into(),
            max_tokens: 512,
            temperature: 0.5,
        };
        let provider = OpenAICompatibleProvider::openai(config).unwrap();
        assert_eq!(provider.id(), "openai");
        assert_eq!(provider.config.model, "gpt-4o-mini");
        assert_eq!(provider.config.max_tokens, 512);
    }

    #[test]
    fn test_convenience_constructor_deepseek() {
        let config = ProviderConfig {
            provider_id: "deepseek".into(),
            endpoint: None,
            api_key: Some("sk-test".into()),
            model: "deepseek-reasoner".into(),
            max_tokens: 2048,
            temperature: 0.0,
        };
        let provider = OpenAICompatibleProvider::deepseek(config).unwrap();
        assert_eq!(provider.id(), "deepseek");
        assert_eq!(provider.config.model, "deepseek-reasoner");
    }

    #[test]
    fn test_convenience_constructor_groq() {
        let config = ProviderConfig {
            provider_id: "groq".into(),
            endpoint: None,
            api_key: Some("gsk-test".into()),
            model: "mixtral-8x7b-32768".into(),
            max_tokens: 4096,
            temperature: 0.7,
        };
        let provider = OpenAICompatibleProvider::groq(config).unwrap();
        assert_eq!(provider.id(), "groq");
        assert_eq!(provider.config.model, "mixtral-8x7b-32768");
    }

    #[test]
    fn test_no_key_errors_with_provider_name() {
        let config = ProviderConfig {
            provider_id: "openai".into(),
            endpoint: None,
            api_key: None,
            model: "gpt-4o".into(),
            max_tokens: 1024,
            temperature: 0.3,
        };
        // Only test if env vars are unset
        if std::env::var("ONECLAW_API_KEY").is_err()
            && std::env::var("OPENAI_API_KEY").is_err()
        {
            let result = OpenAICompatibleProvider::openai(config);
            assert!(result.is_err());
            let err = match result {
                Err(e) => format!("{}", e),
                Ok(_) => panic!("expected error"),
            };
            assert!(err.contains("openai"), "Error should mention provider: {}", err);
            assert!(err.contains("OPENAI_API_KEY"), "Error should mention env var: {}", err);
        }
    }

    #[test]
    fn test_config_key_takes_priority() {
        let config = ProviderConfig {
            provider_id: "openai".into(),
            endpoint: None,
            api_key: Some("sk-explicit-config-key".into()),
            model: "gpt-4o".into(),
            max_tokens: 1024,
            temperature: 0.3,
        };
        let provider = OpenAICompatibleProvider::openai(config).unwrap();
        assert_eq!(
            provider.config.api_key.as_deref(),
            Some("sk-explicit-config-key")
        );
    }

    #[test]
    fn test_custom_endpoint() {
        let provider = OpenAICompatibleProvider::custom(
            "local-llm",
            "Local LLM",
            "http://localhost:8080",
            "LOCAL_API_KEY",
            "local-model",
            ProviderConfig {
                provider_id: "local-llm".into(),
                endpoint: None, // custom() will override
                api_key: Some("test-key".into()),
                model: "local-model".into(),
                max_tokens: 1024,
                temperature: 0.5,
            },
        ).unwrap();
        assert_eq!(provider.id(), "openai-compat"); // custom falls through to default
        assert_eq!(provider.display_name(), "Local LLM");
        assert_eq!(provider.endpoint, "http://localhost:8080");
    }

    // ─── Message conversion tests ───────────────────────────

    #[test]
    fn test_message_conversion_simple() {
        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: "hello".into(),
        }];
        let api_msgs = OpenAICompatibleProvider::to_openai_messages(&messages);
        assert_eq!(api_msgs.len(), 1);
        assert_eq!(api_msgs[0].role, "user");
        assert_eq!(api_msgs[0].content, "hello");
    }

    #[test]
    fn test_message_conversion_with_system() {
        let messages = vec![
            ChatMessage { role: MessageRole::System, content: "Be helpful".into() },
            ChatMessage { role: MessageRole::User, content: "hello".into() },
            ChatMessage { role: MessageRole::Assistant, content: "hi".into() },
            ChatMessage { role: MessageRole::User, content: "bye".into() },
        ];
        let api_msgs = OpenAICompatibleProvider::to_openai_messages(&messages);
        // OpenAI keeps system IN the messages array (unlike Anthropic)
        assert_eq!(api_msgs.len(), 4);
        assert_eq!(api_msgs[0].role, "system");
        assert_eq!(api_msgs[0].content, "Be helpful");
        assert_eq!(api_msgs[1].role, "user");
        assert_eq!(api_msgs[2].role, "assistant");
        assert_eq!(api_msgs[3].role, "user");
    }

    #[test]
    fn test_message_conversion_no_user_errors() {
        let provider = OpenAICompatibleProvider::with_key(
            PRESET_OPENAI, "test-key", "gpt-4o"
        ).unwrap();
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

    // ─── Serialization tests ────────────────────────────────

    #[test]
    fn test_request_serialization() {
        let request = OpenAIRequest {
            model: "gpt-4o".into(),
            messages: vec![
                OpenAIMessage { role: "system".into(), content: "Be helpful".into() },
                OpenAIMessage { role: "user".into(), content: "hello".into() },
            ],
            max_tokens: 1024,
            temperature: Some(0.3),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("gpt-4o"));
        assert!(json.contains("\"role\":\"system\""));
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("1024"));
        assert!(json.contains("0.3"));

        // No system param at top level (unlike Anthropic)
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("system").is_none(), "OpenAI format has no top-level system");
    }

    #[test]
    fn test_response_parsing() {
        let json = r#"{
            "choices": [{
                "message": {"role": "assistant", "content": "Hello!"},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }"#;
        let resp: OpenAIResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.choices.len(), 1);
        assert_eq!(resp.choices[0].message.content, "Hello!");
        assert_eq!(resp.choices[0].finish_reason.as_deref(), Some("stop"));
        let usage = resp.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 5);
        assert_eq!(usage.total_tokens, 15);
    }

    #[test]
    fn test_error_response_parsing() {
        let json = r#"{
            "error": {
                "message": "Invalid API key",
                "type": "invalid_request_error",
                "code": "invalid_api_key"
            }
        }"#;
        let err: OpenAIErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(err.error.message, "Invalid API key");
        assert_eq!(err.error.error_type.as_deref(), Some("invalid_request_error"));
        assert_eq!(err.error.code.as_deref(), Some("invalid_api_key"));
    }

    // ─── default_model_for_provider ─────────────────────────

    #[test]
    fn test_default_model_for_provider() {
        assert_eq!(default_model_for_provider("anthropic"), "claude-sonnet-4-20250514");
        assert_eq!(default_model_for_provider("openai"), "gpt-4o");
        assert_eq!(default_model_for_provider("deepseek"), "deepseek-chat");
        assert_eq!(default_model_for_provider("groq"), "llama-3.3-70b-versatile");
        assert_eq!(default_model_for_provider("ollama"), "llama3.2:1b");
        assert_eq!(default_model_for_provider("google"), "gemini-2.0-flash");
        assert_eq!(default_model_for_provider("gemini"), "gemini-2.0-flash");
        assert_eq!(default_model_for_provider("unknown"), "gpt-4o");
    }

    // ─── Debug output (masked key) ──────────────────────────

    #[test]
    fn test_debug_masks_api_key() {
        let provider = OpenAICompatibleProvider::with_key(
            PRESET_OPENAI, "sk-super-secret-key-12345", "gpt-4o"
        ).unwrap();
        let debug = format!("{:?}", provider);
        assert!(!debug.contains("sk-super-secret-key-12345"),
            "Debug should NOT contain raw key: {}", debug);
        assert!(debug.contains("sk-s...2345"),
            "Debug should contain masked key: {}", debug);
        assert!(debug.contains("openai"));
    }

    // ─── Integration with ReliableProvider + FallbackChain ──

    #[test]
    fn test_with_reliable_wrapper() {
        let provider = OpenAICompatibleProvider::with_key(
            PRESET_OPENAI, "test-key", "gpt-4o"
        ).unwrap();
        let reliable = ReliableProvider::new(provider, 3);
        assert_eq!(reliable.id(), "openai");
        assert!(reliable.is_available());
    }

    #[test]
    fn test_in_fallback_chain() {
        // OpenAI-compat unavailable → fallback to Noop
        let chain = FallbackChain::new(vec![
            Box::new(NoopTestProvider::unavailable()),
            Box::new(NoopTestProvider::available()),
        ]);
        let resp = chain.chat("system", "test").unwrap();
        assert!(resp.content.contains("test"));
        assert_eq!(resp.provider_id, "noop-test");
    }

    // ═══════════════════════════════════════════════════════
    // Live integration tests (require real API keys)
    // ═══════════════════════════════════════════════════════

    #[test]
    #[ignore] // Run: OPENAI_API_KEY=... cargo test test_openai_live_chat -- --ignored
    fn test_openai_live_chat() {
        let key = std::env::var("OPENAI_API_KEY")
            .expect("Set OPENAI_API_KEY to run live test");
        let provider = OpenAICompatibleProvider::with_key(
            PRESET_OPENAI, &key, "gpt-4o-mini"
        ).expect("Provider init failed");

        let response = provider.chat(
            "Reply in exactly 3 words.",
            "Say hello",
        ).expect("API call failed");

        assert!(!response.content.is_empty());
        assert_eq!(response.provider_id, "openai");
        assert!(response.usage.is_some());
        println!("OpenAI live: {}", response.content);
    }

    #[test]
    #[ignore] // Run: DEEPSEEK_API_KEY=... cargo test test_deepseek_live_chat -- --ignored
    fn test_deepseek_live_chat() {
        let key = std::env::var("DEEPSEEK_API_KEY")
            .expect("Set DEEPSEEK_API_KEY to run live test");
        let provider = OpenAICompatibleProvider::with_key(
            PRESET_DEEPSEEK, &key, "deepseek-chat"
        ).expect("Provider init failed");

        let response = provider.chat(
            "Reply in exactly 3 words.",
            "Say hello",
        ).expect("API call failed");

        assert!(!response.content.is_empty());
        assert_eq!(response.provider_id, "deepseek");
        println!("DeepSeek live: {}", response.content);
    }

    #[test]
    #[ignore] // Run: GROQ_API_KEY=... cargo test test_groq_live_chat -- --ignored
    fn test_groq_live_chat() {
        let key = std::env::var("GROQ_API_KEY")
            .expect("Set GROQ_API_KEY to run live test");
        let provider = OpenAICompatibleProvider::with_key(
            PRESET_GROQ, &key, "llama-3.3-70b-versatile"
        ).expect("Provider init failed");

        let response = provider.chat(
            "Reply in exactly 3 words.",
            "Say hello",
        ).expect("API call failed");

        assert!(!response.content.is_empty());
        assert_eq!(response.provider_id, "groq");
        println!("Groq live: {}", response.content);
    }

    #[test]
    #[ignore] // Run: OPENAI_API_KEY=... cargo test test_openai_live_vietnamese -- --ignored
    fn test_openai_live_vietnamese() {
        let key = std::env::var("OPENAI_API_KEY")
            .expect("Set OPENAI_API_KEY to run live test");
        let provider = OpenAICompatibleProvider::with_key(
            PRESET_OPENAI, &key, "gpt-4o-mini"
        ).expect("Provider init failed");

        let response = provider.chat(
            "Bạn là trợ lý chăm sóc sức khoẻ người cao tuổi. Trả lời bằng tiếng Việt.",
            "Bà ngoại tôi bị đau đầu từ sáng, nên làm gì?",
        ).expect("API call failed");

        assert!(!response.content.is_empty());
        assert!(response.content.chars().any(|c| c as u32 > 127));
        println!("OpenAI Vietnamese: {}", response.content);
    }
}
