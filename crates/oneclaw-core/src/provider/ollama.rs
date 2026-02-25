//! Ollama local provider — offline LLM for edge deployment
//!
//! API: POST <http://localhost:11434/api/chat>
//! Docs: <https://github.com/ollama/ollama/blob/main/docs/api.md>
//!
//! Key differences from cloud providers:
//!   - NO API key required (local service)
//!   - Default: http://localhost:11434 (no HTTPS)
//!   - Endpoint: /api/chat (not /v1/chat/completions)
//!   - Health: GET /api/tags (list available models)
//!   - Longer timeout (local inference on RPi = slow)
//!
//! Recommended models for edge (RPi 4, 4GB RAM):
//!   - llama3.2:3b (default — good balance)
//!   - phi3:mini (smaller, faster)
//!   - qwen2.5:3b (multilingual, Vietnamese OK)
//!
//! Heavy models (Mac Mini / desktop):
//!   - llama3.2:7b
//!   - mistral:7b
//!   - deepseek-r1:7b

use crate::error::{OneClawError, Result};
use crate::provider::traits::{
    ChatMessage, MessageRole, Provider, ProviderConfig, ProviderResponse, TokenUsage,
};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, info, warn};

const DEFAULT_ENDPOINT: &str = "http://localhost:11434";
const DEFAULT_MODEL: &str = "llama3.2:3b";
/// Edge devices are slow — 120s timeout (vs 60s for cloud)
const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// Ollama local provider — runs on-device, no internet needed.
///
/// Unlike cloud providers:
/// - NO API key required
/// - HTTP localhost (not HTTPS)
/// - `/api/chat` endpoint (not `/v1/chat/completions`)
/// - Health check via `GET /api/tags`
/// - 120s timeout for slow edge hardware
pub struct OllamaProvider {
    client: Client,
    endpoint: String,
    model: String,
    max_tokens: u32,
    temperature: f32,
}

impl std::fmt::Debug for OllamaProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OllamaProvider")
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .field("temperature", &self.temperature)
            // No API key to mask — Ollama is local
            .finish()
    }
}

// ─── Ollama API types ───────────────────────────────────────

#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
    /// stream = false for blocking response (important!)
    stream: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Deserialize, Debug)]
struct OllamaChatResponse {
    message: OllamaResponseMessage,
    #[serde(default)]
    #[allow(dead_code)]
    done: bool,
    #[serde(default)]
    total_duration: Option<u64>,
    #[serde(default)]
    eval_count: Option<u32>,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
}

#[derive(Deserialize, Debug)]
struct OllamaResponseMessage {
    #[allow(dead_code)]
    role: String,
    content: String,
}

/// Response from GET /api/tags
#[derive(Deserialize, Debug)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Deserialize, Debug)]
struct OllamaModel {
    name: String,
    #[serde(default)]
    #[allow(dead_code)]
    size: Option<u64>,
}

/// Ollama error response
#[derive(Deserialize, Debug)]
struct OllamaErrorResponse {
    error: String,
}

impl OllamaProvider {
    /// Create Ollama provider.
    ///
    /// Unlike cloud providers:
    /// - NO API key needed
    /// - Endpoint defaults to localhost:11434
    /// - Longer timeout for edge hardware
    pub fn new(endpoint: Option<&str>, model: Option<&str>) -> Result<Self> {
        let endpoint = endpoint
            .unwrap_or(DEFAULT_ENDPOINT)
            .trim_end_matches('/')
            .to_string();

        let model = model.unwrap_or(DEFAULT_MODEL).to_string();

        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .map_err(|e| OneClawError::Provider(format!("HTTP client init failed: {}", e)))?;

        info!(
            model = %model,
            endpoint = %endpoint,
            timeout = DEFAULT_TIMEOUT_SECS,
            "Ollama provider initialized (local)"
        );

        Ok(Self {
            client,
            endpoint,
            model,
            max_tokens: 1024,
            temperature: 0.3,
        })
    }

    /// Create from ProviderConfig (for registry integration).
    pub fn from_config(config: &ProviderConfig) -> Result<Self> {
        let endpoint = config.endpoint.as_deref().unwrap_or(DEFAULT_ENDPOINT);
        let model = if config.model.is_empty() {
            DEFAULT_MODEL
        } else {
            &config.model
        };

        let mut provider = Self::new(Some(endpoint), Some(model))?;
        provider.max_tokens = config.max_tokens;
        provider.temperature = config.temperature;
        Ok(provider)
    }

    /// Create with defaults (localhost:11434, llama3.2:3b).
    pub fn default_local() -> Result<Self> {
        Self::new(None, None)
    }

    /// Check if Ollama service is running and model is available.
    ///
    /// Calls GET /api/tags to list available models.
    /// Returns true if service responds AND requested model is in the list.
    pub fn check_health(&self) -> bool {
        let url = format!("{}/api/tags", self.endpoint);

        match self
            .client
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
        {
            Ok(response) if response.status().is_success() => {
                match response.json::<OllamaTagsResponse>() {
                    Ok(tags) => {
                        let base_model = self.model.split(':').next().unwrap_or("");
                        let model_available = tags.models.iter().any(|m| {
                            m.name == self.model
                                || m.name.starts_with(&format!("{}:", base_model))
                        });

                        if !model_available {
                            let available: Vec<&str> =
                                tags.models.iter().map(|m| m.name.as_str()).collect();
                            warn!(
                                requested = %self.model,
                                available = ?available,
                                "Ollama model not found"
                            );
                        }

                        model_available
                    }
                    Err(e) => {
                        warn!(error = %e, "Ollama tags parse failed");
                        false
                    }
                }
            }
            Ok(response) => {
                warn!(status = %response.status(), "Ollama health check failed");
                false
            }
            Err(e) => {
                debug!(error = %e, "Ollama not reachable (may be offline)");
                false
            }
        }
    }

    /// List available models on this Ollama instance.
    pub fn list_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/api/tags", self.endpoint);

        let response = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .map_err(|e| OneClawError::Provider(format!("Ollama list models failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(OneClawError::Provider(format!(
                "Ollama returned {}",
                response.status()
            )));
        }

        let tags: OllamaTagsResponse = response
            .json()
            .map_err(|e| OneClawError::Provider(format!("Parse tags failed: {}", e)))?;

        Ok(tags.models.into_iter().map(|m| m.name).collect())
    }

    /// Internal: execute chat API call.
    fn call_api(&self, request: &OllamaChatRequest) -> Result<ProviderResponse> {
        let url = format!("{}/api/chat", self.endpoint);

        debug!(
            model = %request.model,
            messages = request.messages.len(),
            "Calling Ollama local API"
        );

        let response = self
            .client
            .post(&url)
            .json(request)
            .send()
            .map_err(|e| {
                // Distinguish connection refused vs timeout vs other
                let msg = if e.is_connect() {
                    format!(
                        "Ollama not reachable at {}. Is `ollama serve` running?",
                        self.endpoint
                    )
                } else if e.is_timeout() {
                    format!(
                        "Ollama timed out after {}s. Model may be too large for this hardware.",
                        DEFAULT_TIMEOUT_SECS
                    )
                } else {
                    format!("Ollama request failed: {}", e)
                };
                warn!(error = %e, "Ollama API call failed");
                OneClawError::Provider(msg)
            })?;

        let status = response.status();
        let body = response
            .text()
            .map_err(|e| OneClawError::Provider(format!("Read body failed: {}", e)))?;

        if !status.is_success() {
            let error_msg = match serde_json::from_str::<OllamaErrorResponse>(&body) {
                Ok(err) => format!("Ollama error {}: {}", status, err.error),
                Err(_) => format!("Ollama error {}: {}", status, body),
            };
            warn!(status = %status, "Ollama returned error");
            return Err(OneClawError::Provider(error_msg));
        }

        let api_response: OllamaChatResponse = serde_json::from_str(&body)
            .map_err(|e| OneClawError::Provider(format!("Parse Ollama response failed: {}", e)))?;

        let content = api_response.message.content;

        if content.is_empty() {
            return Err(OneClawError::Provider("Empty response from Ollama".into()));
        }

        // Ollama reports eval_count (output tokens) and prompt_eval_count (input tokens)
        let usage = match (api_response.prompt_eval_count, api_response.eval_count) {
            (Some(prompt), Some(eval)) => Some(TokenUsage {
                prompt_tokens: prompt,
                completion_tokens: eval,
                total_tokens: prompt + eval,
            }),
            _ => None,
        };

        if let Some(duration_ns) = api_response.total_duration {
            let duration_ms = duration_ns / 1_000_000;
            debug!(
                tokens = usage.as_ref().map_or(0, |u| u.total_tokens),
                duration_ms = duration_ms,
                "Ollama response received"
            );
        }

        Ok(ProviderResponse {
            content,
            provider_id: "ollama",
            usage,
        })
    }

    /// Convert ChatMessage list to Ollama format.
    /// Ollama uses same format as OpenAI: system/user/assistant roles in messages.
    fn to_ollama_messages(system: &str, messages: &[ChatMessage]) -> Vec<OllamaMessage> {
        let mut api_messages = Vec::new();

        if !system.is_empty() {
            api_messages.push(OllamaMessage {
                role: "system".into(),
                content: system.into(),
            });
        }

        for msg in messages {
            let role = match msg.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
            };
            api_messages.push(OllamaMessage {
                role: role.into(),
                content: msg.content.clone(),
            });
        }

        api_messages
    }
}

impl Provider for OllamaProvider {
    fn id(&self) -> &'static str {
        "ollama"
    }

    fn display_name(&self) -> &str {
        "Ollama (Local)"
    }

    fn is_available(&self) -> bool {
        // Unlike cloud providers: actually check if service is reachable
        // Fast (5s timeout to localhost) and important for FallbackChain
        self.check_health()
    }

    fn chat(&self, system: &str, user_message: &str) -> Result<ProviderResponse> {
        let mut messages = Vec::new();

        if !system.is_empty() {
            messages.push(OllamaMessage {
                role: "system".into(),
                content: system.into(),
            });
        }

        messages.push(OllamaMessage {
            role: "user".into(),
            content: user_message.into(),
        });

        let request = OllamaChatRequest {
            model: self.model.clone(),
            messages,
            options: Some(OllamaOptions {
                num_predict: Some(self.max_tokens),
                temperature: Some(self.temperature),
            }),
            stream: false, // CRITICAL: blocking mode
        };

        self.call_api(&request)
    }

    fn chat_with_history(
        &self,
        system: &str,
        messages: &[ChatMessage],
    ) -> Result<ProviderResponse> {
        let api_messages = Self::to_ollama_messages(system, messages);

        if api_messages.is_empty() || api_messages.iter().all(|m| m.role == "system") {
            return Err(OneClawError::Provider(
                "No user/assistant messages in history".into(),
            ));
        }

        let request = OllamaChatRequest {
            model: self.model.clone(),
            messages: api_messages,
            options: Some(OllamaOptions {
                num_predict: Some(self.max_tokens),
                temperature: Some(self.temperature),
            }),
            stream: false,
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
    fn test_ollama_new_defaults() {
        let provider = OllamaProvider::new(None, None).unwrap();
        assert_eq!(provider.endpoint, "http://localhost:11434");
        assert_eq!(provider.model, "llama3.2:3b");
        assert_eq!(provider.id(), "ollama");
        assert_eq!(provider.display_name(), "Ollama (Local)");
    }

    #[test]
    fn test_ollama_new_custom_endpoint() {
        let provider =
            OllamaProvider::new(Some("http://192.168.1.100:11434"), Some("phi3:mini")).unwrap();
        assert_eq!(provider.endpoint, "http://192.168.1.100:11434");
        assert_eq!(provider.model, "phi3:mini");
    }

    #[test]
    fn test_ollama_new_trailing_slash_stripped() {
        let provider = OllamaProvider::new(Some("http://localhost:11434/"), None).unwrap();
        assert_eq!(provider.endpoint, "http://localhost:11434");
    }

    #[test]
    fn test_ollama_from_config() {
        let config = ProviderConfig {
            provider_id: "ollama".into(),
            endpoint: Some("http://10.0.0.5:11434".into()),
            api_key: None,
            model: "mistral:7b".into(),
            max_tokens: 2048,
            temperature: 0.5,
        };
        let provider = OllamaProvider::from_config(&config).unwrap();
        assert_eq!(provider.endpoint, "http://10.0.0.5:11434");
        assert_eq!(provider.model, "mistral:7b");
        assert_eq!(provider.max_tokens, 2048);
        assert!((provider.temperature - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_ollama_from_config_empty_model_uses_default() {
        let config = ProviderConfig {
            provider_id: "ollama".into(),
            endpoint: None,
            api_key: None,
            model: String::new(),
            max_tokens: 1024,
            temperature: 0.3,
        };
        let provider = OllamaProvider::from_config(&config).unwrap();
        assert_eq!(provider.model, "llama3.2:3b");
    }

    #[test]
    fn test_ollama_default_local() {
        let provider = OllamaProvider::default_local().unwrap();
        assert_eq!(provider.endpoint, "http://localhost:11434");
        assert_eq!(provider.model, "llama3.2:3b");
        assert_eq!(provider.max_tokens, 1024);
    }

    // ─── No Auth Required ───────────────────────────────────

    #[test]
    fn test_ollama_no_api_key_needed() {
        // Construction NEVER fails due to missing key
        let provider = OllamaProvider::new(None, None).unwrap();
        assert_eq!(provider.id(), "ollama");
        // No key resolution logic — always succeeds

        let config = ProviderConfig {
            provider_id: "ollama".into(),
            endpoint: None,
            api_key: None, // explicitly None — should be fine
            model: "test".into(),
            max_tokens: 100,
            temperature: 0.1,
        };
        let provider2 = OllamaProvider::from_config(&config).unwrap();
        assert_eq!(provider2.id(), "ollama");
    }

    // ─── Message Conversion ─────────────────────────────────

    #[test]
    fn test_ollama_message_conversion_simple() {
        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: "hello".into(),
        }];
        let api_msgs = OllamaProvider::to_ollama_messages("You help", &messages);
        assert_eq!(api_msgs.len(), 2);
        assert_eq!(api_msgs[0].role, "system");
        assert_eq!(api_msgs[0].content, "You help");
        assert_eq!(api_msgs[1].role, "user");
        assert_eq!(api_msgs[1].content, "hello");
    }

    #[test]
    fn test_ollama_message_conversion_no_system() {
        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: "hello".into(),
        }];
        let api_msgs = OllamaProvider::to_ollama_messages("", &messages);
        assert_eq!(api_msgs.len(), 1);
        assert_eq!(api_msgs[0].role, "user");
    }

    #[test]
    fn test_ollama_message_conversion_full_history() {
        let messages = vec![
            ChatMessage {
                role: MessageRole::User,
                content: "hi".into(),
            },
            ChatMessage {
                role: MessageRole::Assistant,
                content: "hello".into(),
            },
            ChatMessage {
                role: MessageRole::User,
                content: "bye".into(),
            },
        ];
        let api_msgs = OllamaProvider::to_ollama_messages("Be helpful", &messages);
        assert_eq!(api_msgs.len(), 4);
        assert_eq!(api_msgs[0].role, "system");
        assert_eq!(api_msgs[1].role, "user");
        assert_eq!(api_msgs[2].role, "assistant");
        assert_eq!(api_msgs[3].role, "user");
    }

    #[test]
    fn test_ollama_chat_with_history_no_user_messages() {
        let provider = OllamaProvider::new(None, None).unwrap();
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

    // ─── Request Serialization ──────────────────────────────

    #[test]
    fn test_ollama_request_serialization() {
        let request = OllamaChatRequest {
            model: "llama3.2:3b".into(),
            messages: vec![
                OllamaMessage {
                    role: "system".into(),
                    content: "Be helpful".into(),
                },
                OllamaMessage {
                    role: "user".into(),
                    content: "hello".into(),
                },
            ],
            options: Some(OllamaOptions {
                num_predict: Some(1024),
                temperature: Some(0.3),
            }),
            stream: false,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("llama3.2:3b"));
        assert!(json.contains("\"role\":\"system\""));
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"stream\":false"), "stream must be false");
        assert!(
            json.contains("num_predict"),
            "Should use num_predict (not max_tokens)"
        );
        assert!(!json.contains("max_tokens"), "Should NOT use max_tokens");

        // Endpoint is /api/chat, NOT /v1/chat/completions (verified at call site)
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(
            parsed.get("system").is_none(),
            "No top-level system param"
        );
    }

    #[test]
    fn test_ollama_request_stream_always_false() {
        let request = OllamaChatRequest {
            model: "test".into(),
            messages: vec![],
            options: None,
            stream: false,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"stream\":false"));
    }

    #[test]
    fn test_ollama_options_serialization() {
        let options = OllamaOptions {
            num_predict: Some(512),
            temperature: Some(0.7),
        };
        let json = serde_json::to_string(&options).unwrap();
        assert!(json.contains("\"num_predict\":512"));
        assert!(json.contains("\"temperature\":0.7"));
        assert!(!json.contains("max_tokens"));
    }

    // ─── Response Parsing ───────────────────────────────────

    #[test]
    fn test_ollama_response_parsing() {
        let json = r#"{
            "message": {"role": "assistant", "content": "Hello!"},
            "done": true,
            "total_duration": 500000000,
            "eval_count": 5,
            "prompt_eval_count": 10
        }"#;
        let resp: OllamaChatResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.message.content, "Hello!");
        assert_eq!(resp.eval_count, Some(5));
        assert_eq!(resp.prompt_eval_count, Some(10));
        assert_eq!(resp.total_duration, Some(500000000));
    }

    #[test]
    fn test_ollama_response_no_token_counts() {
        let json = r#"{
            "message": {"role": "assistant", "content": "Hi there"},
            "done": true
        }"#;
        let resp: OllamaChatResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.message.content, "Hi there");
        assert!(resp.eval_count.is_none());
        assert!(resp.prompt_eval_count.is_none());
        // usage should be None (not an error)
    }

    #[test]
    fn test_ollama_error_response_parsing() {
        let json = r#"{"error": "model 'xyz' not found, try pulling it first"}"#;
        let err: OllamaErrorResponse = serde_json::from_str(json).unwrap();
        assert!(err.error.contains("model 'xyz' not found"));
    }

    // ─── Tags/Models Response Parsing ───────────────────────

    #[test]
    fn test_ollama_tags_response_parsing() {
        let json = r#"{
            "models": [
                {"name": "llama3.2:3b", "size": 2000000000},
                {"name": "phi3:mini", "size": 1500000000}
            ]
        }"#;
        let resp: OllamaTagsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.models.len(), 2);
        assert_eq!(resp.models[0].name, "llama3.2:3b");
        assert_eq!(resp.models[1].name, "phi3:mini");
        assert_eq!(resp.models[0].size, Some(2000000000));
    }

    #[test]
    fn test_ollama_tags_response_empty() {
        let json = r#"{"models": []}"#;
        let resp: OllamaTagsResponse = serde_json::from_str(json).unwrap();
        assert!(resp.models.is_empty());
    }

    // ─── Health Check (unreachable endpoint) ────────────────

    #[test]
    fn test_ollama_is_available_returns_false_when_unreachable() {
        // Point to a port that's definitely not running Ollama
        let provider = OllamaProvider::new(Some("http://127.0.0.1:19999"), None).unwrap();
        assert!(!provider.check_health());
        // is_available uses check_health
        // Note: is_available() would also be false, but it's slow (5s timeout)
        // so we test check_health() directly
    }

    // ─── Debug Output ───────────────────────────────────────

    #[test]
    fn test_ollama_debug_no_key_field() {
        let provider = OllamaProvider::new(None, None).unwrap();
        let debug = format!("{:?}", provider);
        assert!(debug.contains("OllamaProvider"));
        assert!(debug.contains("localhost:11434"));
        assert!(debug.contains("llama3.2:3b"));
        // No api_key field at all (Ollama is local)
        assert!(!debug.contains("api_key"));
    }

    // ─── Provider Trait Compliance ───────────────────────────

    #[test]
    fn test_ollama_is_provider() {
        let provider = OllamaProvider::new(None, None).unwrap();
        let boxed: Box<dyn Provider> = Box::new(provider);
        assert_eq!(boxed.id(), "ollama");
        assert_eq!(boxed.display_name(), "Ollama (Local)");
    }

    #[test]
    fn test_ollama_in_fallback_chain() {
        // Type-check: OllamaProvider works in FallbackChain
        // Use NoopTestProvider as stand-in (Ollama won't connect in test)
        let chain = FallbackChain::new(vec![
            Box::new(NoopTestProvider::unavailable()), // cloud "down"
            Box::new(NoopTestProvider::available()),    // stand-in for ollama
        ]);
        let resp = chain.chat("system", "test").unwrap();
        assert!(resp.content.contains("test"));
    }

    #[test]
    fn test_ollama_with_reliable_wrapper() {
        let provider = OllamaProvider::new(None, None).unwrap();
        let reliable = ReliableProvider::new(provider, 3);
        assert_eq!(reliable.id(), "ollama");
        assert_eq!(reliable.display_name(), "Ollama (Local)");
    }

    // ═══════════════════════════════════════════════════════
    // Live tests (require Ollama running locally)
    // ═══════════════════════════════════════════════════════

    #[test]
    #[ignore] // Run: cargo test test_ollama_live_chat -- --ignored
    fn test_ollama_live_chat() {
        let provider = OllamaProvider::default_local().expect("Init failed");

        if !provider.check_health() {
            println!("Ollama not running, skipping live test");
            return;
        }

        let response = provider
            .chat("Reply in exactly 3 words.", "Say hello")
            .expect("Chat failed");

        assert!(!response.content.is_empty());
        assert_eq!(response.provider_id, "ollama");
        println!("Ollama response: {}", response.content);
    }

    #[test]
    #[ignore]
    fn test_ollama_live_vietnamese() {
        let provider = OllamaProvider::default_local().expect("Init failed");

        if !provider.check_health() {
            println!("Ollama not running, skipping");
            return;
        }

        let response = provider
            .chat(
                "Trả lời bằng tiếng Việt, ngắn gọn.",
                "Bà bị đau đầu, nên làm gì?",
            )
            .expect("Chat failed");

        assert!(!response.content.is_empty());
        println!("Ollama Vietnamese: {}", response.content);
    }

    #[test]
    #[ignore]
    fn test_ollama_live_list_models() {
        let provider = OllamaProvider::default_local().expect("Init failed");

        match provider.list_models() {
            Ok(models) => {
                assert!(!models.is_empty(), "Should have at least one model");
                println!("Available models: {:?}", models);
            }
            Err(e) => println!("Ollama not running: {}", e),
        }
    }
}
