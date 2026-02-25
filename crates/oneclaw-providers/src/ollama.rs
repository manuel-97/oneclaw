//! Ollama LLM Provider — Local inference via Ollama REST API
//!
//! Ollama API: POST <http://localhost:11434/api/chat>
//! Docs: <https://github.com/ollama/ollama/blob/main/docs/api.md>

use oneclaw_core::orchestrator::provider::*;
use oneclaw_core::error::{OneClawError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};
use std::time::Instant;

/// LLM provider that connects to a local Ollama instance.
pub struct OllamaProvider {
    base_url: String,
    default_model: String,
    client: reqwest::Client,
}

impl OllamaProvider {
    /// Create a new Ollama provider with the given base URL and default model.
    pub fn new(base_url: impl Into<String>, default_model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            default_model: default_model.into(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Create an Ollama provider from an `OllamaConfig`.
    pub fn from_config(config: &oneclaw_core::config::OllamaConfig) -> Self {
        Self::new(&config.url, &config.model)
    }
}

// Ollama API types
#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Serialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    message: OllamaResponseMessage,
    model: String,
    #[serde(default)]
    eval_count: u32,
    #[serde(default)]
    prompt_eval_count: u32,
}

#[derive(Deserialize)]
struct OllamaResponseMessage {
    content: String,
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModelInfo>,
}

#[derive(Deserialize)]
struct OllamaModelInfo {
    name: String,
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn name(&self) -> &str { "ollama" }

    async fn is_available(&self) -> Result<bool> {
        let url = format!("{}/api/tags", self.base_url);
        match self.client.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(e) => {
                debug!("Ollama not available: {}", e);
                Ok(false)
            }
        }
    }

    async fn list_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/api/tags", self.base_url);
        let resp = self.client.get(&url).send().await
            .map_err(|e| OneClawError::Orchestrator(format!("Ollama list models: {}", e)))?;

        if !resp.status().is_success() {
            return Err(OneClawError::Orchestrator(
                format!("Ollama API error: {}", resp.status())
            ));
        }

        let tags: OllamaTagsResponse = resp.json().await
            .map_err(|e| OneClawError::Orchestrator(format!("Ollama parse: {}", e)))?;

        Ok(tags.models.into_iter().map(|m| m.name).collect())
    }

    async fn chat(&self, request: &LlmRequest) -> Result<LlmResponse> {
        let url = format!("{}/api/chat", self.base_url);
        let model = if request.model.is_empty() {
            &self.default_model
        } else {
            &request.model
        };

        let ollama_messages: Vec<OllamaMessage> = request.messages.iter()
            .map(|m| OllamaMessage {
                role: match m.role {
                    MessageRole::System => "system".into(),
                    MessageRole::User => "user".into(),
                    MessageRole::Assistant => "assistant".into(),
                },
                content: m.content.clone(),
            })
            .collect();

        let options = if request.temperature.is_some() || request.max_tokens.is_some() {
            Some(OllamaOptions {
                temperature: request.temperature,
                num_predict: request.max_tokens,
            })
        } else {
            None
        };

        let ollama_req = OllamaChatRequest {
            model: model.to_string(),
            messages: ollama_messages,
            stream: false,
            options,
        };

        info!(model = %model, messages = request.messages.len(), "Ollama chat request");
        let start = Instant::now();

        let resp = self.client.post(&url)
            .json(&ollama_req)
            .send()
            .await
            .map_err(|e| OneClawError::Orchestrator(format!("Ollama request failed: {}", e)))?;

        let latency_ms = start.elapsed().as_millis() as u64;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(OneClawError::Orchestrator(
                format!("Ollama API error {}: {}", status, body)
            ));
        }

        let ollama_resp: OllamaChatResponse = resp.json().await
            .map_err(|e| OneClawError::Orchestrator(format!("Ollama parse response: {}", e)))?;

        info!(
            model = %ollama_resp.model,
            latency_ms = latency_ms,
            eval_tokens = ollama_resp.eval_count,
            "Ollama chat response"
        );

        Ok(LlmResponse {
            content: ollama_resp.message.content,
            model: ollama_resp.model,
            usage: Some(TokenUsage {
                prompt_tokens: ollama_resp.prompt_eval_count,
                completion_tokens: ollama_resp.eval_count,
                total_tokens: ollama_resp.prompt_eval_count + ollama_resp.eval_count,
            }),
            latency_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_provider_creation() {
        let provider = OllamaProvider::new("http://localhost:11434", "llama3.2:1b");
        assert_eq!(provider.name(), "ollama");
    }

    #[tokio::test]
    async fn test_ollama_availability_graceful_fail() {
        // Connect to a port that's definitely not Ollama
        let provider = OllamaProvider::new("http://localhost:19999", "test");
        // Should return Ok(false), not Err
        let available = provider.is_available().await.unwrap();
        assert!(!available);
    }
}
