//! Embedding provider trait and implementations for generating text embeddings.
//!
//! Used by the vector memory system to convert text into dense vectors
//! for semantic search. Supports both local (Ollama) and cloud (OpenAI) models.

use crate::error::{OneClawError, Result};
use crate::memory::vector::Embedding;

// ==================== CONFIG ====================

/// Configuration for an embedding provider.
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    /// Provider type: "ollama" or "openai".
    pub provider: String,
    /// Model name, e.g. "nomic-embed-text", "text-embedding-3-small".
    pub model: String,
    /// API endpoint (for Ollama: `http://localhost:11434`, for OpenAI: `https://api.openai.com`).
    pub endpoint: String,
    /// API key (required for OpenAI, ignored for Ollama).
    pub api_key: Option<String>,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".into(),
            model: "nomic-embed-text".into(),
            endpoint: "http://localhost:11434".into(),
            api_key: None,
            timeout_secs: 30,
        }
    }
}

// ==================== TRAIT ====================

/// Trait for generating text embeddings.
///
/// Implementations must be Send + Sync for use across threads.
/// All methods are synchronous (matching the sync Provider pattern).
pub trait EmbeddingProvider: Send + Sync {
    /// Provider identifier, e.g. "ollama", "openai".
    fn id(&self) -> &str;

    /// Generate embedding for a single text.
    fn embed(&self, text: &str) -> Result<Embedding>;

    /// Generate embeddings for multiple texts in one request (batch).
    /// Default implementation calls embed() in a loop.
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Embedding>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }

    /// Expected embedding dimensions for the configured model.
    fn dimensions(&self) -> usize;

    /// Check if the embedding provider is reachable.
    fn is_available(&self) -> bool;

    /// Model identifier string for storage (e.g. "ollama:nomic-embed-text").
    fn model_id(&self) -> String {
        format!("{}:{}", self.id(), self.model_name())
    }

    /// Model name.
    fn model_name(&self) -> &str;
}

// ==================== BUILDER ====================

use super::embedding_ollama::OllamaEmbedding;
use super::embedding_openai::OpenAIEmbedding;

/// Build an EmbeddingProvider from config.
pub fn build_embedding_provider(config: &EmbeddingConfig) -> Result<Box<dyn EmbeddingProvider>> {
    match config.provider.as_str() {
        "ollama" => {
            let provider = OllamaEmbedding::new(config)?;
            Ok(Box::new(provider))
        }
        "openai" => {
            let provider = OpenAIEmbedding::new(config)?;
            Ok(Box::new(provider))
        }
        other => Err(OneClawError::Provider(format!(
            "Unknown embedding provider: '{}'. Supported: ollama, openai", other
        ))),
    }
}

// ==================== CONFIG PARSER ====================

/// Parse EmbeddingConfig from a TOML value.
///
/// Expected config:
/// ```toml
/// [embedding]
/// provider = "ollama"
/// model = "nomic-embed-text"
/// endpoint = "http://localhost:11434"
/// # api_key = "sk-..."
/// # api_key_env = "OPENAI_API_KEY"
/// # timeout_secs = 30
/// ```
pub fn parse_embedding_config(table: &toml::Value) -> Result<EmbeddingConfig> {
    let section = table.get("embedding")
        .ok_or_else(|| OneClawError::Config("Missing [embedding] section".into()))?;

    let provider = section.get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("ollama")
        .to_string();

    let model = section.get("model")
        .and_then(|v| v.as_str())
        .unwrap_or(match provider.as_str() {
            "openai" => "text-embedding-3-small",
            _ => "nomic-embed-text",
        })
        .to_string();

    let endpoint = section.get("endpoint")
        .and_then(|v| v.as_str())
        .unwrap_or(match provider.as_str() {
            "openai" => "https://api.openai.com",
            _ => "http://localhost:11434",
        })
        .to_string();

    // API key: direct value or from env var
    let api_key = section.get("api_key")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
            section.get("api_key_env")
                .and_then(|v| v.as_str())
                .and_then(|env_name| std::env::var(env_name).ok())
        });

    let timeout_secs = section.get("timeout_secs")
        .and_then(|v| v.as_integer())
        .unwrap_or(30) as u64;

    Ok(EmbeddingConfig {
        provider,
        model,
        endpoint,
        api_key,
        timeout_secs,
    })
}

// ==================== TESTS ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::embedding_ollama::ollama_model_dimensions;
    use crate::provider::embedding_openai::openai_model_dimensions;

    // ── EmbeddingConfig ──

    #[test]
    fn test_default_config() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.provider, "ollama");
        assert_eq!(config.model, "nomic-embed-text");
        assert_eq!(config.endpoint, "http://localhost:11434");
        assert!(config.api_key.is_none());
        assert_eq!(config.timeout_secs, 30);
    }

    // ── Ollama dimensions ──

    #[test]
    fn test_ollama_known_dimensions() {
        assert_eq!(ollama_model_dimensions("nomic-embed-text"), 768);
        assert_eq!(ollama_model_dimensions("all-minilm"), 384);
        assert_eq!(ollama_model_dimensions("mxbai-embed-large"), 1024);
        assert_eq!(ollama_model_dimensions("snowflake-arctic-embed"), 1024);
    }

    #[test]
    fn test_ollama_unknown_model_defaults() {
        assert_eq!(ollama_model_dimensions("some-new-model"), 768);
    }

    // ── OpenAI dimensions ──

    #[test]
    fn test_openai_known_dimensions() {
        assert_eq!(openai_model_dimensions("text-embedding-3-small"), 1536);
        assert_eq!(openai_model_dimensions("text-embedding-3-large"), 3072);
        assert_eq!(openai_model_dimensions("text-embedding-ada-002"), 1536);
    }

    #[test]
    fn test_openai_unknown_model_defaults() {
        assert_eq!(openai_model_dimensions("some-new-model"), 1536);
    }

    // ── Config parsing ──

    #[test]
    fn test_parse_ollama_config() {
        let toml_str = r#"
            [embedding]
            provider = "ollama"
            model = "nomic-embed-text"
            endpoint = "http://localhost:11434"
            timeout_secs = 60
        "#;
        let value: toml::Value = toml_str.parse().unwrap();
        let config = parse_embedding_config(&value).unwrap();
        assert_eq!(config.provider, "ollama");
        assert_eq!(config.model, "nomic-embed-text");
        assert_eq!(config.endpoint, "http://localhost:11434");
        assert_eq!(config.timeout_secs, 60);
        assert!(config.api_key.is_none());
    }

    #[test]
    fn test_parse_openai_config() {
        let toml_str = r#"
            [embedding]
            provider = "openai"
            model = "text-embedding-3-small"
            api_key = "sk-test-key"
        "#;
        let value: toml::Value = toml_str.parse().unwrap();
        let config = parse_embedding_config(&value).unwrap();
        assert_eq!(config.provider, "openai");
        assert_eq!(config.model, "text-embedding-3-small");
        assert_eq!(config.api_key, Some("sk-test-key".into()));
        assert_eq!(config.endpoint, "https://api.openai.com");
    }

    #[test]
    fn test_parse_config_defaults() {
        let toml_str = r#"
            [embedding]
            provider = "ollama"
        "#;
        let value: toml::Value = toml_str.parse().unwrap();
        let config = parse_embedding_config(&value).unwrap();
        assert_eq!(config.model, "nomic-embed-text");
        assert_eq!(config.endpoint, "http://localhost:11434");
        assert_eq!(config.timeout_secs, 30);
    }

    #[test]
    fn test_parse_openai_config_defaults() {
        let toml_str = r#"
            [embedding]
            provider = "openai"
            api_key = "sk-x"
        "#;
        let value: toml::Value = toml_str.parse().unwrap();
        let config = parse_embedding_config(&value).unwrap();
        assert_eq!(config.model, "text-embedding-3-small");
        assert_eq!(config.endpoint, "https://api.openai.com");
    }

    #[test]
    fn test_parse_missing_section() {
        let toml_str = r#"
            [other]
            foo = "bar"
        "#;
        let value: toml::Value = toml_str.parse().unwrap();
        assert!(parse_embedding_config(&value).is_err());
    }

    // ── Builder ──

    #[test]
    fn test_build_ollama_provider() {
        let config = EmbeddingConfig::default();
        let provider = build_embedding_provider(&config).unwrap();
        assert_eq!(provider.id(), "ollama");
        assert_eq!(provider.dimensions(), 768);
        assert_eq!(provider.model_name(), "nomic-embed-text");
        assert_eq!(provider.model_id(), "ollama:nomic-embed-text");
    }

    #[test]
    fn test_build_openai_requires_key() {
        let config = EmbeddingConfig {
            provider: "openai".into(),
            model: "text-embedding-3-small".into(),
            endpoint: "https://api.openai.com".into(),
            api_key: None,
            timeout_secs: 30,
        };
        let result = build_embedding_provider(&config);
        // Should fail if no OPENAI_API_KEY env var set
        if std::env::var("OPENAI_API_KEY").is_err() {
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_build_openai_with_key() {
        let config = EmbeddingConfig {
            provider: "openai".into(),
            model: "text-embedding-3-small".into(),
            endpoint: "https://api.openai.com".into(),
            api_key: Some("sk-test-key".into()),
            timeout_secs: 30,
        };
        let provider = build_embedding_provider(&config).unwrap();
        assert_eq!(provider.id(), "openai");
        assert_eq!(provider.dimensions(), 1536);
        assert_eq!(provider.model_name(), "text-embedding-3-small");
        assert_eq!(provider.model_id(), "openai:text-embedding-3-small");
    }

    #[test]
    fn test_build_unknown_provider() {
        let config = EmbeddingConfig {
            provider: "unknown".into(),
            ..EmbeddingConfig::default()
        };
        assert!(build_embedding_provider(&config).is_err());
    }

    // ── model_id format ──

    #[test]
    fn test_model_id_format() {
        let config = EmbeddingConfig::default();
        let provider = build_embedding_provider(&config).unwrap();
        assert!(provider.model_id().contains(':'));
        assert!(provider.model_id().starts_with("ollama:"));
    }

    // ── Batch empty ──

    #[test]
    fn test_embed_batch_empty() {
        let config = EmbeddingConfig::default();
        let provider = build_embedding_provider(&config).unwrap();
        let result = provider.embed_batch(&[]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    // ── OllamaEmbedding construction ──

    #[test]
    fn test_ollama_with_defaults() {
        let provider = OllamaEmbedding::with_defaults().unwrap();
        assert_eq!(provider.id(), "ollama");
        assert_eq!(provider.dimensions(), 768);
    }

    #[test]
    fn test_ollama_custom_model() {
        let config = EmbeddingConfig {
            model: "all-minilm".into(),
            ..EmbeddingConfig::default()
        };
        let provider = OllamaEmbedding::new(&config).unwrap();
        assert_eq!(provider.dimensions(), 384);
        assert_eq!(provider.model_name(), "all-minilm");
    }
}

// ═══ Integration tests (require running services, mark #[ignore]) ═══

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    #[ignore] // Requires: ollama running + nomic-embed-text pulled
    fn test_ollama_embed_real() {
        let config = EmbeddingConfig::default();
        let provider = build_embedding_provider(&config).unwrap();
        assert!(provider.is_available(), "Ollama not running");

        let embedding = provider.embed("Hello, world!").unwrap();
        assert_eq!(embedding.dim(), 768);
        assert_eq!(embedding.values.len(), 768);
        assert!(embedding.values.iter().any(|v| *v != 0.0));
    }

    #[test]
    #[ignore]
    fn test_ollama_embed_batch_real() {
        let config = EmbeddingConfig::default();
        let provider = build_embedding_provider(&config).unwrap();

        let texts = &["hello", "world", "test"];
        let embeddings = provider.embed_batch(texts).unwrap();
        assert_eq!(embeddings.len(), 3);
        for emb in &embeddings {
            assert_eq!(emb.dim(), 768);
        }
    }

    #[test]
    #[ignore]
    fn test_ollama_embed_similarity() {
        use crate::memory::vector::cosine_similarity;

        let config = EmbeddingConfig::default();
        let provider = build_embedding_provider(&config).unwrap();

        let emb_a = provider.embed("The temperature is very hot today").unwrap();
        let emb_b = provider.embed("It is warm and sunny outside").unwrap();
        let emb_c = provider.embed("I love programming in Rust").unwrap();

        let sim_ab = cosine_similarity(&emb_a.values, &emb_b.values);
        let sim_ac = cosine_similarity(&emb_a.values, &emb_c.values);

        assert!(sim_ab > sim_ac,
            "Similar topics should have higher similarity: ab={} ac={}", sim_ab, sim_ac);
    }

    #[test]
    #[ignore] // Requires: OPENAI_API_KEY env var
    fn test_openai_embed_real() {
        let config = EmbeddingConfig {
            provider: "openai".into(),
            model: "text-embedding-3-small".into(),
            endpoint: "https://api.openai.com".into(),
            api_key: None,
            timeout_secs: 30,
        };
        let provider = build_embedding_provider(&config).unwrap();

        let embedding = provider.embed("Hello, world!").unwrap();
        assert_eq!(embedding.dim(), 1536);
        assert_eq!(embedding.values.len(), 1536);
    }
}
