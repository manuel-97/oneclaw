//! OpenAI-Compatible LLM Provider
//!
//! Works with: OpenAI, Azure OpenAI, Together AI, Groq, local vLLM,
//! and any server implementing the OpenAI chat completions API.
//!
//! API: POST {base_url}/chat/completions

use oneclaw_core::orchestrator::provider::*;
use oneclaw_core::error::{OneClawError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};
use std::time::Instant;

/// LLM provider for OpenAI-compatible chat completion APIs.
pub struct OpenAICompatProvider {
    base_url: String,
    api_key: String,
    default_model: String,
    client: reqwest::Client,
}

impl OpenAICompatProvider {
    /// Create a new provider with the given base URL, API key, and default model.
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        default_model: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            default_model: default_model.into(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Create a provider from an `OpenAIConfig`, preferring env-var API keys.
    pub fn from_config(config: &oneclaw_core::config::OpenAIConfig) -> Self {
        // API key from config, but prefer env var
        let api_key = std::env::var("OPENAI_API_KEY")
            .or_else(|_| std::env::var("ONECLAW_OPENAI_KEY"))
            .unwrap_or_else(|_| config.api_key.clone());

        Self::new(&config.base_url, api_key, &config.model)
    }
}

// OpenAI API types
#[derive(Serialize)]
struct OpenAIChatRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Serialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OpenAIChatResponse {
    choices: Vec<OpenAIChoice>,
    model: String,
    usage: Option<OpenAIUsage>,
}

#[derive(Deserialize)]
struct OpenAIChoice {
    message: OpenAIResponseMessage,
}

#[derive(Deserialize)]
struct OpenAIResponseMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Deserialize)]
struct OpenAIModelsResponse {
    data: Vec<OpenAIModelInfo>,
}

#[derive(Deserialize)]
struct OpenAIModelInfo {
    id: String,
}

#[derive(Deserialize)]
struct OpenAIErrorResponse {
    error: Option<OpenAIErrorDetail>,
}

#[derive(Deserialize)]
struct OpenAIErrorDetail {
    message: String,
}

#[async_trait]
impl LlmProvider for OpenAICompatProvider {
    fn name(&self) -> &str { "openai" }

    async fn is_available(&self) -> Result<bool> {
        let url = format!("{}/models", self.base_url);
        let mut req = self.client.get(&url);
        if !self.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.api_key));
        }
        match req.send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(e) => {
                debug!("OpenAI-compat not available: {}", e);
                Ok(false)
            }
        }
    }

    async fn list_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/models", self.base_url);
        let mut req = self.client.get(&url);
        if !self.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let resp = req.send().await
            .map_err(|e| OneClawError::Orchestrator(format!("OpenAI list models: {}", e)))?;

        if !resp.status().is_success() {
            return Err(OneClawError::Orchestrator(
                format!("OpenAI API error: {}", resp.status())
            ));
        }

        let models: OpenAIModelsResponse = resp.json().await
            .map_err(|e| OneClawError::Orchestrator(format!("OpenAI parse: {}", e)))?;

        Ok(models.data.into_iter().map(|m| m.id).collect())
    }

    async fn chat(&self, request: &LlmRequest) -> Result<LlmResponse> {
        let url = format!("{}/chat/completions", self.base_url);
        let model = if request.model.is_empty() {
            &self.default_model
        } else {
            &request.model
        };

        let messages: Vec<OpenAIMessage> = request.messages.iter()
            .map(|m| OpenAIMessage {
                role: match m.role {
                    MessageRole::System => "system".into(),
                    MessageRole::User => "user".into(),
                    MessageRole::Assistant => "assistant".into(),
                },
                content: m.content.clone(),
            })
            .collect();

        let openai_req = OpenAIChatRequest {
            model: model.to_string(),
            messages,
            max_tokens: request.max_tokens,
            temperature: request.temperature,
        };

        info!(model = %model, "OpenAI-compat chat request");
        let start = Instant::now();

        let mut http_req = self.client.post(&url).json(&openai_req);
        if !self.api_key.is_empty() {
            http_req = http_req.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let resp = http_req.send().await
            .map_err(|e| OneClawError::Orchestrator(format!("OpenAI request failed: {}", e)))?;

        let latency_ms = start.elapsed().as_millis() as u64;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            // Try to parse error message
            let err_msg = serde_json::from_str::<OpenAIErrorResponse>(&body)
                .ok()
                .and_then(|e| e.error)
                .map(|e| e.message)
                .unwrap_or(body);
            return Err(OneClawError::Orchestrator(
                format!("OpenAI API error {}: {}", status, err_msg)
            ));
        }

        let openai_resp: OpenAIChatResponse = resp.json().await
            .map_err(|e| OneClawError::Orchestrator(format!("OpenAI parse response: {}", e)))?;

        let content = openai_resp.choices.first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        let usage = openai_resp.usage.map(|u| TokenUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });

        info!(
            model = %openai_resp.model,
            latency_ms = latency_ms,
            tokens = usage.as_ref().map(|u| u.total_tokens).unwrap_or(0),
            "OpenAI-compat chat response"
        );

        Ok(LlmResponse {
            content,
            model: openai_resp.model,
            usage,
            latency_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_provider_creation() {
        let provider = OpenAICompatProvider::new(
            "https://api.openai.com/v1",
            "test-key",
            "gpt-4o-mini",
        );
        assert_eq!(provider.name(), "openai");
    }

    #[tokio::test]
    async fn test_openai_availability_graceful_fail() {
        let provider = OpenAICompatProvider::new(
            "http://localhost:19998",
            "",
            "test",
        );
        let available = provider.is_available().await.unwrap();
        assert!(!available);
    }
}
