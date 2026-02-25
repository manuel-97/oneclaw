//! OpenAI embedding provider — cloud-based, high quality.
//!
//! Uses OpenAI's /v1/embeddings endpoint.
//! Also compatible with any OpenAI-compatible API (e.g. local inference servers).
//! Default model: text-embedding-3-small (1536 dimensions).

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::error::{OneClawError, Result};
use crate::memory::vector::Embedding;
use super::embedding::{EmbeddingConfig, EmbeddingProvider};

/// Known OpenAI embedding model dimensions.
pub fn openai_model_dimensions(model: &str) -> usize {
    match model {
        "text-embedding-3-small" => 1536,
        "text-embedding-3-large" => 3072,
        "text-embedding-ada-002" => 1536,
        _ => 1536, // default assumption
    }
}

/// OpenAI embedding provider.
pub struct OpenAIEmbedding {
    client: Client,
    endpoint: String,
    model: String,
    api_key: String,
    dimensions: usize,
}

impl OpenAIEmbedding {
    /// Create from config. Requires API key (from config or OPENAI_API_KEY env).
    pub fn new(config: &EmbeddingConfig) -> Result<Self> {
        let api_key = config.api_key.clone()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .ok_or_else(|| OneClawError::Provider(
                "OpenAI embedding requires API key (config or OPENAI_API_KEY env)".into()
            ))?;

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| OneClawError::Provider(format!("HTTP client error: {}", e)))?;

        let dimensions = openai_model_dimensions(&config.model);

        Ok(Self {
            client,
            endpoint: config.endpoint.trim_end_matches('/').to_string(),
            model: config.model.clone(),
            api_key,
            dimensions,
        })
    }
}

#[derive(Serialize)]
struct OpenAIEmbedRequest<'a> {
    model: &'a str,
    input: Vec<&'a str>,
}

#[derive(Deserialize)]
struct OpenAIEmbedResponse {
    data: Vec<OpenAIEmbedData>,
}

#[derive(Deserialize)]
struct OpenAIEmbedData {
    embedding: Vec<f32>,
}

impl EmbeddingProvider for OpenAIEmbedding {
    fn id(&self) -> &str { "openai" }

    fn embed(&self, text: &str) -> Result<Embedding> {
        let results = self.embed_batch(&[text])?;
        results.into_iter().next()
            .ok_or_else(|| OneClawError::Provider("OpenAI returned empty embeddings".into()))
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Embedding>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let url = format!("{}/v1/embeddings", self.endpoint);

        let request = OpenAIEmbedRequest {
            model: &self.model,
            input: texts.to_vec(),
        };

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .map_err(|e| OneClawError::Provider(format!("OpenAI embed request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(OneClawError::Provider(format!(
                "OpenAI embed error {}: {}", status, body
            )));
        }

        let result: OpenAIEmbedResponse = response.json()
            .map_err(|e| OneClawError::Provider(format!("OpenAI embed parse error: {}", e)))?;

        Ok(result.data
            .into_iter()
            .map(|d| Embedding::new(d.embedding, self.model_id()))
            .collect())
    }

    fn dimensions(&self) -> usize { self.dimensions }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    fn model_name(&self) -> &str { &self.model }
}
