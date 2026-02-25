//! Ollama embedding provider — local, offline-capable.
//!
//! Uses Ollama's /api/embed endpoint.
//! Default model: nomic-embed-text (768 dimensions, fast, good quality).

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::error::{OneClawError, Result};
use crate::memory::vector::Embedding;
use super::embedding::{EmbeddingConfig, EmbeddingProvider};

/// Known Ollama embedding model dimensions.
pub fn ollama_model_dimensions(model: &str) -> usize {
    match model {
        "nomic-embed-text" => 768,
        "mxbai-embed-large" => 1024,
        "all-minilm" => 384,
        "snowflake-arctic-embed" => 1024,
        _ => 768, // default assumption
    }
}

/// Ollama embedding provider.
pub struct OllamaEmbedding {
    client: Client,
    endpoint: String,
    model: String,
    dimensions: usize,
}

impl OllamaEmbedding {
    /// Create from config.
    pub fn new(config: &EmbeddingConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| OneClawError::Provider(format!("HTTP client error: {}", e)))?;

        let dimensions = ollama_model_dimensions(&config.model);

        Ok(Self {
            client,
            endpoint: config.endpoint.trim_end_matches('/').to_string(),
            model: config.model.clone(),
            dimensions,
        })
    }

    /// Create with default config (Ollama localhost, nomic-embed-text).
    pub fn with_defaults() -> Result<Self> {
        Self::new(&EmbeddingConfig::default())
    }
}

#[derive(Serialize)]
struct OllamaEmbedRequest<'a> {
    model: &'a str,
    input: Vec<&'a str>,
}

#[derive(Deserialize)]
struct OllamaEmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

impl EmbeddingProvider for OllamaEmbedding {
    fn id(&self) -> &str { "ollama" }

    fn embed(&self, text: &str) -> Result<Embedding> {
        let url = format!("{}/api/embed", self.endpoint);

        let request = OllamaEmbedRequest {
            model: &self.model,
            input: vec![text],
        };

        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .map_err(|e| OneClawError::Provider(format!("Ollama embed request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(OneClawError::Provider(format!(
                "Ollama embed error {}: {}", status, body
            )));
        }

        let result: OllamaEmbedResponse = response.json()
            .map_err(|e| OneClawError::Provider(format!("Ollama embed parse error: {}", e)))?;

        let values = result.embeddings.into_iter().next()
            .ok_or_else(|| OneClawError::Provider("Ollama returned empty embeddings".into()))?;

        Ok(Embedding::new(values, self.model_id()))
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Embedding>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let url = format!("{}/api/embed", self.endpoint);

        let request = OllamaEmbedRequest {
            model: &self.model,
            input: texts.to_vec(),
        };

        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .map_err(|e| OneClawError::Provider(format!("Ollama batch embed failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(OneClawError::Provider(format!(
                "Ollama embed error {}: {}", status, body
            )));
        }

        let result: OllamaEmbedResponse = response.json()
            .map_err(|e| OneClawError::Provider(format!("Ollama embed parse error: {}", e)))?;

        Ok(result.embeddings
            .into_iter()
            .map(|values| Embedding::new(values, self.model_id()))
            .collect())
    }

    fn dimensions(&self) -> usize { self.dimensions }

    fn is_available(&self) -> bool {
        self.client
            .get(format!("{}/api/tags", self.endpoint))
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    fn model_name(&self) -> &str { &self.model }
}
