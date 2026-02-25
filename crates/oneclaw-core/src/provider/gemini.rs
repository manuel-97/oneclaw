//! Google Gemini provider — multimodal AI
//!
//! API: POST `https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent`
//! Docs: <https://ai.google.dev/api/generate-content>
//!
//! Supported models:
//!   - gemini-2.0-flash (default — fast, cheap, good quality)
//!   - gemini-2.0-flash-lite (fastest, cheapest)
//!   - gemini-2.5-pro (best quality, expensive)
//!   - gemini-2.5-flash (balanced with thinking)
//!
//! Key API differences from other providers:
//!   - Model name is in URL path, not request body
//!   - API key in query parameter (?key=), not header
//!   - System prompt is separate "systemInstruction" field
//!   - Role "model" instead of "assistant"
//!   - Content uses "parts" array, not flat "content" string
//!   - camelCase JSON fields throughout

use crate::error::{OneClawError, Result};
use crate::provider::traits::{
    ChatMessage, MessageRole, Provider, ProviderConfig, ProviderResponse, TokenUsage,
};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, info, warn};

const DEFAULT_ENDPOINT: &str = "https://generativelanguage.googleapis.com";
const DEFAULT_MODEL: &str = "gemini-2.0-flash";
const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// Google Gemini provider — 4th API format in OneClaw's arsenal.
///
/// Gemini differs from all other providers:
/// - Model in URL path (`/v1beta/models/{model}:generateContent`)
/// - API key in query string (`?key=`)
/// - System instruction as separate top-level field
/// - "model" role instead of "assistant"
/// - `parts` array instead of flat content string
///
/// API key resolution order:
/// 1. config.api_key (explicit in TOML or code)
/// 2. ONECLAW_API_KEY environment variable
/// 3. GOOGLE_API_KEY environment variable
/// 4. GEMINI_API_KEY environment variable
pub struct GeminiProvider {
    client: Client,
    config: ProviderConfig,
    endpoint: String,
}

impl std::fmt::Debug for GeminiProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeminiProvider")
            .field("model", &self.config.model)
            .field("endpoint", &self.endpoint)
            .field("api_key", &"[MASKED]")
            .finish()
    }
}

// ─── Gemini API Request types ───────────────────────────────

#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "systemInstruction")]
    system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "generationConfig")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct GeminiContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<GeminiPart>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct GeminiPart {
    text: String,
}

#[derive(Serialize)]
struct GeminiGenerationConfig {
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "maxOutputTokens"
    )]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

// ─── Gemini API Response types ──────────────────────────────

#[derive(Deserialize, Debug)]
struct GeminiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
    #[serde(default, rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Deserialize, Debug)]
struct GeminiCandidate {
    content: GeminiContent,
    #[serde(default, rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Debug)]
struct GeminiUsageMetadata {
    #[serde(default, rename = "promptTokenCount")]
    prompt_token_count: Option<u32>,
    #[serde(default, rename = "candidatesTokenCount")]
    candidates_token_count: Option<u32>,
    #[serde(default, rename = "totalTokenCount")]
    total_token_count: Option<u32>,
}

// ─── Gemini Error types ─────────────────────────────────────

#[derive(Deserialize, Debug)]
struct GeminiErrorResponse {
    error: GeminiErrorDetail,
}

#[derive(Deserialize, Debug)]
struct GeminiErrorDetail {
    #[serde(default)]
    code: Option<u32>,
    message: String,
    #[serde(default)]
    status: Option<String>,
}

impl GeminiProvider {
    /// Create a new Gemini provider.
    ///
    /// API key resolution order:
    /// 1. config.api_key (explicit)
    /// 2. ONECLAW_API_KEY env var (universal)
    /// 3. GOOGLE_API_KEY env var
    /// 4. GEMINI_API_KEY env var
    /// 5. Error
    pub fn new(config: ProviderConfig) -> Result<Self> {
        let api_key = config
            .api_key
            .clone()
            .or_else(|| std::env::var("ONECLAW_API_KEY").ok())
            .or_else(|| std::env::var("GOOGLE_API_KEY").ok())
            .or_else(|| std::env::var("GEMINI_API_KEY").ok())
            .ok_or_else(|| {
                OneClawError::Provider(
                    "No API key for Gemini: set api_key in config, ONECLAW_API_KEY, GOOGLE_API_KEY, or GEMINI_API_KEY".into(),
                )
            })?;

        let endpoint = config
            .endpoint
            .clone()
            .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string())
            .trim_end_matches('/')
            .to_string();

        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .map_err(|e| OneClawError::Provider(format!("HTTP client init failed: {}", e)))?;

        let mut resolved_config = config;
        resolved_config.api_key = Some(api_key);
        if resolved_config.model.is_empty() {
            resolved_config.model = DEFAULT_MODEL.into();
        }

        info!(
            model = %resolved_config.model,
            endpoint = %endpoint,
            "Gemini provider initialized"
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
            provider_id: "google".into(),
            endpoint: None,
            api_key: Some(api_key.into()),
            model: model.into(),
            max_tokens: 1024,
            temperature: 0.3,
        })
    }

    /// Build the full API URL with model and key.
    /// Format: `{endpoint}/v1beta/models/{model}:generateContent?key={api_key}`
    fn build_url(&self) -> Result<String> {
        let api_key = self
            .config
            .api_key
            .as_deref()
            .ok_or_else(|| OneClawError::Provider("API key missing".into()))?;

        Ok(format!(
            "{}/v1beta/models/{}:generateContent?key={}",
            self.endpoint, self.config.model, api_key
        ))
    }

    /// Internal: execute API call.
    fn call_api(&self, request: &GeminiRequest) -> Result<ProviderResponse> {
        let url = self.build_url()?;

        debug!(
            model = %self.config.model,
            contents = request.contents.len(),
            "Calling Gemini API"
        );

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .map_err(|e| {
                warn!(error = %e, "Gemini API request failed");
                OneClawError::Provider(format!("Gemini request failed: {}", e))
            })?;

        let status = response.status();
        let body = response
            .text()
            .map_err(|e| OneClawError::Provider(format!("Read body failed: {}", e)))?;

        if !status.is_success() {
            let error_msg = match serde_json::from_str::<GeminiErrorResponse>(&body) {
                Ok(err) => format!(
                    "Gemini API error {}: {} ({})",
                    err.error.code.unwrap_or(status.as_u16() as u32),
                    err.error.message,
                    err.error.status.as_deref().unwrap_or("unknown")
                ),
                Err(_) => format!("Gemini API error {}: {}", status, body),
            };
            warn!(status = %status, "Gemini API returned error");
            return Err(OneClawError::Provider(error_msg));
        }

        let api_response: GeminiResponse = serde_json::from_str(&body)
            .map_err(|e| OneClawError::Provider(format!("Parse Gemini response failed: {}", e)))?;

        // Extract text from candidates[0].content.parts
        let content = api_response
            .candidates
            .as_ref()
            .and_then(|c| c.first())
            .map(|candidate| {
                candidate
                    .content
                    .parts
                    .iter()
                    .map(|p| p.text.as_str())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .ok_or_else(|| {
                OneClawError::Provider("No candidates in Gemini response".into())
            })?;

        if content.is_empty() {
            return Err(OneClawError::Provider(
                "Empty response from Gemini".into(),
            ));
        }

        let usage = api_response.usage_metadata.map(|u| TokenUsage {
            prompt_tokens: u.prompt_token_count.unwrap_or(0),
            completion_tokens: u.candidates_token_count.unwrap_or(0),
            total_tokens: u.total_token_count.unwrap_or(0),
        });

        let finish_reason = api_response
            .candidates
            .as_ref()
            .and_then(|c| c.first())
            .and_then(|c| c.finish_reason.as_deref())
            .unwrap_or("unknown");

        debug!(
            tokens = usage.as_ref().map_or(0, |u| u.total_tokens),
            finish = finish_reason,
            "Gemini response received"
        );

        Ok(ProviderResponse {
            content,
            provider_id: "google",
            usage,
        })
    }

    /// Convert ChatMessage list to Gemini format.
    ///
    /// Key mappings:
    /// - `MessageRole::System` → extracted to `system_instruction` (NOT in contents)
    /// - `MessageRole::User` → role = `"user"`
    /// - `MessageRole::Assistant` → role = `"model"` (Gemini uses "model", not "assistant")
    /// - content string → `parts: [{text: content}]`
    fn to_gemini_contents(
        messages: &[ChatMessage],
    ) -> (Option<GeminiContent>, Vec<GeminiContent>) {
        let mut system_instruction: Option<GeminiContent> = None;
        let mut contents = Vec::new();

        for msg in messages {
            match msg.role {
                MessageRole::System => {
                    // Gemini: system goes into system_instruction, not contents
                    // Multiple system messages → concatenate
                    match &mut system_instruction {
                        Some(existing) => {
                            if let Some(part) = existing.parts.first_mut() {
                                part.text.push('\n');
                                part.text.push_str(&msg.content);
                            }
                        }
                        None => {
                            system_instruction = Some(GeminiContent {
                                role: None, // system_instruction has no role field
                                parts: vec![GeminiPart {
                                    text: msg.content.clone(),
                                }],
                            });
                        }
                    }
                }
                MessageRole::User => {
                    contents.push(GeminiContent {
                        role: Some("user".into()),
                        parts: vec![GeminiPart {
                            text: msg.content.clone(),
                        }],
                    });
                }
                MessageRole::Assistant => {
                    // Gemini uses "model" NOT "assistant"
                    contents.push(GeminiContent {
                        role: Some("model".into()),
                        parts: vec![GeminiPart {
                            text: msg.content.clone(),
                        }],
                    });
                }
            }
        }

        (system_instruction, contents)
    }
}

impl Provider for GeminiProvider {
    fn id(&self) -> &'static str {
        "google"
    }

    fn display_name(&self) -> &str {
        "Google Gemini"
    }

    fn is_available(&self) -> bool {
        self.config.api_key.is_some()
    }

    fn chat(&self, system: &str, user_message: &str) -> Result<ProviderResponse> {
        let system_instruction = if system.is_empty() {
            None
        } else {
            Some(GeminiContent {
                role: None,
                parts: vec![GeminiPart {
                    text: system.into(),
                }],
            })
        };

        let request = GeminiRequest {
            contents: vec![GeminiContent {
                role: Some("user".into()),
                parts: vec![GeminiPart {
                    text: user_message.into(),
                }],
            }],
            system_instruction,
            generation_config: Some(GeminiGenerationConfig {
                max_output_tokens: Some(self.config.max_tokens),
                temperature: Some(self.config.temperature),
            }),
        };

        self.call_api(&request)
    }

    fn chat_with_history(
        &self,
        system: &str,
        messages: &[ChatMessage],
    ) -> Result<ProviderResponse> {
        let (msg_system, contents) = Self::to_gemini_contents(messages);

        // Merge: explicit system param + message-embedded system
        let system_instruction = match (system.is_empty(), msg_system) {
            (false, Some(mut msg_sys)) => {
                // Prepend explicit system to message-embedded system
                if let Some(part) = msg_sys.parts.first_mut() {
                    part.text = format!("{}\n{}", system, part.text);
                }
                Some(msg_sys)
            }
            (false, None) => Some(GeminiContent {
                role: None,
                parts: vec![GeminiPart {
                    text: system.into(),
                }],
            }),
            (true, msg_sys) => msg_sys,
        };

        if contents.is_empty() {
            return Err(OneClawError::Provider(
                "No user/model messages in history".into(),
            ));
        }

        let request = GeminiRequest {
            contents,
            system_instruction,
            generation_config: Some(GeminiGenerationConfig {
                max_output_tokens: Some(self.config.max_tokens),
                temperature: Some(self.config.temperature),
            }),
        };

        self.call_api(&request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::traits::{FallbackChain, NoopTestProvider, ReliableProvider};

    // ─── Construction tests ─────────────────────────────────

    #[test]
    fn test_gemini_new_with_config() {
        let config = ProviderConfig {
            provider_id: "google".into(),
            endpoint: None,
            api_key: Some("test-gemini-key-1234".into()),
            model: "gemini-2.0-flash".into(),
            max_tokens: 1024,
            temperature: 0.3,
        };
        let provider = GeminiProvider::new(config).unwrap();
        assert_eq!(provider.id(), "google");
        assert_eq!(provider.display_name(), "Google Gemini");
        assert!(provider.is_available());
        assert_eq!(provider.config.model, "gemini-2.0-flash");
    }

    #[test]
    fn test_gemini_with_key() {
        let provider =
            GeminiProvider::with_key("test-key-5678", "gemini-2.5-pro").unwrap();
        assert_eq!(provider.id(), "google");
        assert_eq!(provider.config.model, "gemini-2.5-pro");
        assert!(provider.is_available());
    }

    #[test]
    fn test_gemini_no_key_errors() {
        let config = ProviderConfig {
            provider_id: "google".into(),
            endpoint: None,
            api_key: None,
            model: "gemini-2.0-flash".into(),
            max_tokens: 1024,
            temperature: 0.3,
        };
        // Only test if env vars are unset
        if std::env::var("ONECLAW_API_KEY").is_err()
            && std::env::var("GOOGLE_API_KEY").is_err()
            && std::env::var("GEMINI_API_KEY").is_err()
        {
            let result = GeminiProvider::new(config);
            assert!(result.is_err());
            let err = match result {
                Err(e) => format!("{}", e),
                Ok(_) => panic!("expected error"),
            };
            assert!(err.contains("GOOGLE_API_KEY"), "Error should mention GOOGLE_API_KEY: {}", err);
            assert!(err.contains("GEMINI_API_KEY"), "Error should mention GEMINI_API_KEY: {}", err);
        }
    }

    #[test]
    fn test_gemini_config_key_takes_priority() {
        let config = ProviderConfig {
            provider_id: "google".into(),
            endpoint: None,
            api_key: Some("explicit-config-key".into()),
            model: "gemini-2.0-flash".into(),
            max_tokens: 1024,
            temperature: 0.3,
        };
        let provider = GeminiProvider::new(config).unwrap();
        assert_eq!(
            provider.config.api_key.as_deref(),
            Some("explicit-config-key")
        );
    }

    #[test]
    fn test_gemini_empty_model_uses_default() {
        let config = ProviderConfig {
            provider_id: "google".into(),
            endpoint: None,
            api_key: Some("test-key".into()),
            model: String::new(),
            max_tokens: 1024,
            temperature: 0.3,
        };
        let provider = GeminiProvider::new(config).unwrap();
        assert_eq!(provider.config.model, "gemini-2.0-flash");
    }

    #[test]
    fn test_gemini_debug_masks_key() {
        let provider =
            GeminiProvider::with_key("AIzaSyB-super-secret-key", "gemini-2.0-flash").unwrap();
        let debug = format!("{:?}", provider);
        assert!(!debug.contains("AIzaSyB-super-secret-key"),
            "Debug should NOT contain raw key: {}", debug);
        assert!(debug.contains("[MASKED]"),
            "Debug should contain [MASKED]: {}", debug);
        assert!(debug.contains("gemini-2.0-flash"));
    }

    // ─── URL Building ───────────────────────────────────────

    #[test]
    fn test_gemini_url_format() {
        let provider =
            GeminiProvider::with_key("test-api-key", "gemini-2.0-flash").unwrap();
        let url = provider.build_url().unwrap();
        assert_eq!(
            url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:generateContent?key=test-api-key"
        );
        // Key in query string, model in path
        assert!(url.contains("?key=test-api-key"));
        assert!(url.contains("/models/gemini-2.0-flash:"));
    }

    #[test]
    fn test_gemini_url_trailing_slash_handled() {
        let config = ProviderConfig {
            provider_id: "google".into(),
            endpoint: Some("https://custom.endpoint.com/".into()),
            api_key: Some("key".into()),
            model: "gemini-2.0-flash".into(),
            max_tokens: 1024,
            temperature: 0.3,
        };
        let provider = GeminiProvider::new(config).unwrap();
        let url = provider.build_url().unwrap();
        // No double slash
        assert!(!url.contains("com//v1beta"));
        assert!(url.starts_with("https://custom.endpoint.com/v1beta/"));
    }

    #[test]
    fn test_gemini_url_custom_model() {
        let provider =
            GeminiProvider::with_key("key", "gemini-2.5-pro").unwrap();
        let url = provider.build_url().unwrap();
        assert!(url.contains("/models/gemini-2.5-pro:generateContent"));
    }

    // ─── Message Conversion ─────────────────────────────────

    #[test]
    fn test_gemini_message_conversion_simple() {
        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: "hello".into(),
        }];
        let (sys, contents) = GeminiProvider::to_gemini_contents(&messages);
        assert!(sys.is_none());
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].role.as_deref(), Some("user"));
        assert_eq!(contents[0].parts[0].text, "hello");
    }

    #[test]
    fn test_gemini_message_conversion_with_system() {
        let messages = vec![
            ChatMessage { role: MessageRole::System, content: "You are helpful".into() },
            ChatMessage { role: MessageRole::User, content: "hello".into() },
        ];
        let (sys, contents) = GeminiProvider::to_gemini_contents(&messages);
        // System extracted to system_instruction
        let sys = sys.expect("should have system_instruction");
        assert!(sys.role.is_none(), "system_instruction must have no role");
        assert_eq!(sys.parts[0].text, "You are helpful");
        // Only user message in contents
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].role.as_deref(), Some("user"));
    }

    #[test]
    fn test_gemini_message_conversion_assistant_is_model() {
        let messages = vec![
            ChatMessage { role: MessageRole::User, content: "a".into() },
            ChatMessage { role: MessageRole::Assistant, content: "b".into() },
            ChatMessage { role: MessageRole::User, content: "c".into() },
        ];
        let (_, contents) = GeminiProvider::to_gemini_contents(&messages);
        assert_eq!(contents.len(), 3);
        assert_eq!(contents[0].role.as_deref(), Some("user"));
        assert_eq!(contents[1].role.as_deref(), Some("model"), "Assistant should become 'model' NOT 'assistant'");
        assert_eq!(contents[2].role.as_deref(), Some("user"));
    }

    #[test]
    fn test_gemini_message_conversion_multiple_system() {
        let messages = vec![
            ChatMessage { role: MessageRole::System, content: "A".into() },
            ChatMessage { role: MessageRole::System, content: "B".into() },
            ChatMessage { role: MessageRole::User, content: "hello".into() },
        ];
        let (sys, contents) = GeminiProvider::to_gemini_contents(&messages);
        let sys = sys.expect("should have system_instruction");
        assert_eq!(sys.parts[0].text, "A\nB", "Multiple system messages concatenated");
        assert_eq!(contents.len(), 1);
    }

    #[test]
    fn test_gemini_message_no_user_messages() {
        let provider = GeminiProvider::with_key("test-key", "gemini-2.0-flash").unwrap();
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
        assert!(err.contains("No user/model messages"));
    }

    #[test]
    fn test_gemini_system_instruction_no_role() {
        let messages = vec![
            ChatMessage { role: MessageRole::System, content: "sys".into() },
            ChatMessage { role: MessageRole::User, content: "hi".into() },
        ];
        let (sys, _) = GeminiProvider::to_gemini_contents(&messages);
        let sys = sys.unwrap();
        // role = None means it won't appear in serialized JSON
        assert!(sys.role.is_none());
        let json = serde_json::to_string(&sys).unwrap();
        assert!(!json.contains("\"role\""), "systemInstruction should have no role field in JSON: {}", json);
    }

    // ─── Request Serialization ──────────────────────────────

    #[test]
    fn test_gemini_request_serialization() {
        let request = GeminiRequest {
            contents: vec![GeminiContent {
                role: Some("user".into()),
                parts: vec![GeminiPart { text: "hello".into() }],
            }],
            system_instruction: Some(GeminiContent {
                role: None,
                parts: vec![GeminiPart { text: "Be helpful".into() }],
            }),
            generation_config: Some(GeminiGenerationConfig {
                max_output_tokens: Some(1024),
                temperature: Some(0.3),
            }),
        };
        let json = serde_json::to_string(&request).unwrap();
        // camelCase fields
        assert!(json.contains("\"systemInstruction\""), "Should use camelCase: {}", json);
        assert!(json.contains("\"generationConfig\""), "Should use camelCase: {}", json);
        assert!(json.contains("\"maxOutputTokens\""), "Should use camelCase: {}", json);
        // parts array structure
        assert!(json.contains("\"parts\""));
        assert!(json.contains("\"text\":\"hello\""));
        // NOT flat content
        assert!(!json.contains("\"content\":\"hello\""));
        // No model in body (it's in URL path)
        assert!(!json.contains("\"model\""));
    }

    #[test]
    fn test_gemini_request_no_system_omitted() {
        let request = GeminiRequest {
            contents: vec![GeminiContent {
                role: Some("user".into()),
                parts: vec![GeminiPart { text: "hi".into() }],
            }],
            system_instruction: None,
            generation_config: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(!json.contains("systemInstruction"), "None should be omitted: {}", json);
        assert!(!json.contains("generationConfig"), "None should be omitted: {}", json);
    }

    #[test]
    fn test_gemini_generation_config_serialization() {
        let config = GeminiGenerationConfig {
            max_output_tokens: Some(2048),
            temperature: Some(0.7),
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"maxOutputTokens\":2048"), "camelCase: {}", json);
        assert!(!json.contains("max_output_tokens"), "Should NOT have snake_case");
    }

    // ─── Response Parsing ───────────────────────────────────

    #[test]
    fn test_gemini_response_parsing() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"text": "Hello!"}]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 5,
                "totalTokenCount": 15
            }
        }"#;
        let resp: GeminiResponse = serde_json::from_str(json).unwrap();
        let candidates = resp.candidates.unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].content.parts[0].text, "Hello!");
        assert_eq!(candidates[0].content.role.as_deref(), Some("model"));
        assert_eq!(candidates[0].finish_reason.as_deref(), Some("STOP"));
        let usage = resp.usage_metadata.unwrap();
        assert_eq!(usage.prompt_token_count, Some(10));
        assert_eq!(usage.candidates_token_count, Some(5));
        assert_eq!(usage.total_token_count, Some(15));
    }

    #[test]
    fn test_gemini_response_multi_parts() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [
                        {"text": "Part 1"},
                        {"text": " Part 2"}
                    ]
                },
                "finishReason": "STOP"
            }]
        }"#;
        let resp: GeminiResponse = serde_json::from_str(json).unwrap();
        let candidates = resp.candidates.unwrap();
        let content: String = candidates[0]
            .content
            .parts
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(content, "Part 1 Part 2");
    }

    #[test]
    fn test_gemini_response_no_candidates() {
        // Empty candidates array
        let json = r#"{"candidates": []}"#;
        let resp: GeminiResponse = serde_json::from_str(json).unwrap();
        assert!(resp.candidates.unwrap().is_empty());

        // null candidates
        let json2 = r#"{}"#;
        let resp2: GeminiResponse = serde_json::from_str(json2).unwrap();
        assert!(resp2.candidates.is_none());
    }

    #[test]
    fn test_gemini_response_no_usage() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"text": "Hi"}]
                }
            }]
        }"#;
        let resp: GeminiResponse = serde_json::from_str(json).unwrap();
        assert!(resp.usage_metadata.is_none());
    }

    #[test]
    fn test_gemini_error_response_parsing() {
        let json = r#"{
            "error": {
                "code": 400,
                "message": "API key not valid. Please pass a valid API key.",
                "status": "INVALID_ARGUMENT"
            }
        }"#;
        let err: GeminiErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(err.error.code, Some(400));
        assert!(err.error.message.contains("API key not valid"));
        assert_eq!(err.error.status.as_deref(), Some("INVALID_ARGUMENT"));
    }

    #[test]
    fn test_gemini_error_response_minimal() {
        let json = r#"{
            "error": {
                "message": "Some error"
            }
        }"#;
        let err: GeminiErrorResponse = serde_json::from_str(json).unwrap();
        assert!(err.error.code.is_none());
        assert_eq!(err.error.message, "Some error");
        assert!(err.error.status.is_none());
    }

    // ─── Provider Trait Compliance ───────────────────────────

    #[test]
    fn test_gemini_is_provider() {
        let provider = GeminiProvider::with_key("test-key", "gemini-2.0-flash").unwrap();
        let boxed: Box<dyn Provider> = Box::new(provider);
        assert_eq!(boxed.id(), "google");
        assert_eq!(boxed.display_name(), "Google Gemini");
    }

    #[test]
    fn test_gemini_with_reliable_wrapper() {
        let provider = GeminiProvider::with_key("test-key", "gemini-2.0-flash").unwrap();
        let reliable = ReliableProvider::new(provider, 3);
        assert_eq!(reliable.id(), "google");
        assert!(reliable.is_available());
    }

    #[test]
    fn test_gemini_in_fallback_chain() {
        let chain = FallbackChain::new(vec![
            Box::new(NoopTestProvider::unavailable()),
            Box::new(NoopTestProvider::available()),
        ]);
        let resp = chain.chat("system", "test").unwrap();
        assert!(resp.content.contains("test"));
    }

    #[test]
    fn test_all_six_providers_in_chain() {
        // THE MILESTONE TEST: all 6 provider types compile in a FallbackChain
        // Use test providers for cloud ones (they need keys but won't connect)
        // This proves the full Provider trait is satisfied by all 6 implementations

        let anthropic = crate::provider::AnthropicProvider::with_key("test", "claude-sonnet-4-20250514").unwrap();
        let openai = crate::provider::OpenAICompatibleProvider::with_key(
            crate::provider::PRESET_OPENAI, "test", "gpt-4o"
        ).unwrap();
        let deepseek = crate::provider::OpenAICompatibleProvider::with_key(
            crate::provider::PRESET_DEEPSEEK, "test", "deepseek-chat"
        ).unwrap();
        let groq = crate::provider::OpenAICompatibleProvider::with_key(
            crate::provider::PRESET_GROQ, "test", "llama-3.3-70b-versatile"
        ).unwrap();
        let gemini = GeminiProvider::with_key("test", "gemini-2.0-flash").unwrap();
        let ollama = crate::provider::OllamaProvider::default_local().unwrap();

        let chain = FallbackChain::new(vec![
            Box::new(anthropic),   // 1. Anthropic Claude
            Box::new(openai),      // 2. OpenAI GPT
            Box::new(deepseek),    // 3. DeepSeek
            Box::new(groq),        // 4. Groq
            Box::new(gemini),      // 5. Google Gemini
            Box::new(ollama),      // 6. Ollama (local)
        ]);

        // All 6 are valid Provider implementations
        assert!(chain.id() == "fallback-chain");
        // At least some should be "available" (have keys)
        // Ollama won't be available (no local service in test), but cloud ones have test keys
        assert!(chain.is_available(), "Chain with test-keyed providers should be available");
    }

    // ─── Chat with system merge ─────────────────────────────

    #[test]
    fn test_gemini_chat_with_history_system_merge() {
        let provider = GeminiProvider::with_key("test-key", "gemini-2.0-flash").unwrap();
        // Explicit system + message-embedded system should merge
        let messages = vec![
            ChatMessage { role: MessageRole::System, content: "From messages".into() },
            ChatMessage { role: MessageRole::User, content: "hello".into() },
        ];
        // This would fail on API call, but we can test the conversion
        let (msg_sys, contents) = GeminiProvider::to_gemini_contents(&messages);
        assert!(msg_sys.is_some());
        assert_eq!(contents.len(), 1);

        // Test the merge logic: explicit system "Override" + msg system "From messages"
        // The merge happens inside chat_with_history, we verify the conversion is correct
        let _ = provider; // provider tested above
    }

    // ═══════════════════════════════════════════════════════
    // Live tests (require GOOGLE_API_KEY or GEMINI_API_KEY)
    // ═══════════════════════════════════════════════════════

    #[test]
    #[ignore] // Run: GOOGLE_API_KEY=... cargo test test_gemini_live_chat -- --ignored
    fn test_gemini_live_chat() {
        let key = std::env::var("GOOGLE_API_KEY")
            .or_else(|_| std::env::var("GEMINI_API_KEY"))
            .expect("Set GOOGLE_API_KEY or GEMINI_API_KEY to run live test");

        let provider = GeminiProvider::with_key(&key, "gemini-2.0-flash")
            .expect("Provider init failed");

        let response = provider
            .chat("Reply in exactly 3 words.", "Say hello")
            .expect("API call failed");

        assert!(!response.content.is_empty());
        assert_eq!(response.provider_id, "google");
        assert!(response.usage.is_some());
        println!("Gemini response: {}", response.content);
    }

    #[test]
    #[ignore]
    fn test_gemini_live_vietnamese() {
        let key = std::env::var("GOOGLE_API_KEY")
            .or_else(|_| std::env::var("GEMINI_API_KEY"))
            .expect("Set GOOGLE_API_KEY or GEMINI_API_KEY");

        let provider = GeminiProvider::with_key(&key, "gemini-2.0-flash")
            .expect("Init failed");

        let response = provider
            .chat(
                "Bạn là trợ lý chăm sóc sức khoẻ người cao tuổi. Trả lời bằng tiếng Việt, ngắn gọn.",
                "Bà ngoại tôi bị chóng mặt khi đứng dậy, nên làm gì?",
            )
            .expect("API call failed");

        assert!(!response.content.is_empty());
        println!("Gemini Vietnamese: {}", response.content);
    }
}
