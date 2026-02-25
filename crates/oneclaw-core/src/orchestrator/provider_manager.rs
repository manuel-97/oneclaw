//! Provider Manager — Manages multiple LLM providers
//! The bridge between ModelRouter (which picks a provider name) and actual providers.

use crate::orchestrator::provider::{LlmProvider, LlmRequest, LlmResponse, NoopProvider};
use crate::orchestrator::fallback::DegradationMode;
use crate::error::{OneClawError, Result};
use std::collections::HashMap;
use tracing::{info, warn};

/// Manages multiple LLM providers with fallback and degradation support
pub struct ProviderManager {
    providers: HashMap<String, Box<dyn LlmProvider>>,
    default_provider: String,
    degradation: DegradationMode,
}

impl ProviderManager {
    /// Create a new provider manager with the given default provider name
    pub fn new(default_provider: impl Into<String>) -> Self {
        let mut mgr = Self {
            providers: HashMap::new(),
            default_provider: default_provider.into(),
            degradation: DegradationMode::default(),
        };
        // Always have noop as fallback
        mgr.register(Box::new(NoopProvider));
        mgr
    }

    /// Register a provider
    pub fn register(&mut self, provider: Box<dyn LlmProvider>) {
        let name = provider.name().to_string();
        info!(provider = %name, "Registered LLM provider");
        self.providers.insert(name, provider);
    }

    /// Get the default provider name
    pub fn default_provider(&self) -> &str {
        &self.default_provider
    }

    /// Check which providers are available (async)
    pub async fn check_availability(&self) -> HashMap<String, bool> {
        let mut result = HashMap::new();
        for (name, p) in &self.providers {
            let available = p.is_available().await.unwrap_or(false);
            result.insert(name.clone(), available);
        }
        result
    }

    /// Send a request to a specific provider (async)
    pub async fn chat(&self, provider_name: &str, request: &LlmRequest) -> Result<LlmResponse> {
        if self.degradation == DegradationMode::Emergency {
            return Err(OneClawError::Orchestrator(
                "Emergency mode: no LLM calls allowed".into()
            ));
        }

        let provider = self.providers.get(provider_name)
            .ok_or_else(|| OneClawError::Orchestrator(
                format!("Provider '{}' not registered", provider_name)
            ))?;

        provider.chat(request).await
    }

    /// Send to default provider with automatic fallback to noop (async)
    pub async fn chat_default(&self, request: &LlmRequest) -> Result<LlmResponse> {
        match self.chat(&self.default_provider, request).await {
            Ok(resp) => Ok(resp),
            Err(e) => {
                warn!(
                    provider = %self.default_provider,
                    error = %e,
                    "Default provider failed, falling back to noop"
                );
                self.chat("noop", request).await
            }
        }
    }

    /// Set degradation mode
    pub fn set_degradation(&mut self, mode: DegradationMode) {
        info!(mode = ?mode, "Degradation mode changed");
        self.degradation = mode;
    }

    /// Get current degradation mode
    pub fn degradation(&self) -> DegradationMode {
        self.degradation
    }

    /// List all registered providers
    pub fn list_providers(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_manager_creation() {
        let mgr = ProviderManager::new("ollama");
        assert_eq!(mgr.default_provider(), "ollama");
        assert!(mgr.list_providers().contains(&"noop"));
    }

    #[tokio::test]
    async fn test_chat_noop() {
        let mgr = ProviderManager::new("noop");
        let req = LlmRequest::simple("noop-model", "hello");
        let resp = mgr.chat("noop", &req).await.unwrap();
        assert!(resp.content.contains("hello"));
    }

    #[tokio::test]
    async fn test_chat_default_fallback() {
        let mgr = ProviderManager::new("nonexistent");
        let req = LlmRequest::simple("model", "test");
        // Default provider doesn't exist, should fall back to noop
        let resp = mgr.chat_default(&req).await.unwrap();
        assert!(resp.content.contains("test"));
    }

    #[tokio::test]
    async fn test_emergency_mode_blocks() {
        let mut mgr = ProviderManager::new("noop");
        mgr.set_degradation(DegradationMode::Emergency);
        let req = LlmRequest::simple("model", "test");
        assert!(mgr.chat("noop", &req).await.is_err());
    }

    #[tokio::test]
    async fn test_check_availability() {
        let mgr = ProviderManager::new("noop");
        let avail = mgr.check_availability().await;
        assert_eq!(avail.get("noop"), Some(&true));
    }
}
