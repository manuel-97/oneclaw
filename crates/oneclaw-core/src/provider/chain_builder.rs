//! Provider Chain Builder — config-driven FallbackChain assembly
//!
//! Builds a FallbackChain from `[provider]` TOML config:
//!   - Primary provider (required)
//!   - Fallback providers (optional, tried in order)
//!   - Per-provider API keys via `[provider.keys]`
//!   - Per-provider retry wrapping via `max_retries`
//!   - Graceful degradation: no key → skip provider, all fail → None (offline)

use crate::config::ProviderConfigToml;
use crate::error::{OneClawError, Result};
use crate::provider::traits::{
    FallbackChain, Provider, ProviderConfig, ReliableProvider,
};
use tracing::{info, warn};

/// Build a complete provider chain from config.
///
/// Returns:
/// - `Ok(Some(provider))` — single or chain ready to use
/// - `Ok(None)` — graceful degradation (no providers available)
/// - Never panics, never returns Err (logs warnings instead)
pub fn build_provider_chain(config: &ProviderConfigToml) -> Option<Box<dyn Provider>> {
    // 1. Build primary
    let primary = build_single_provider(&config.primary, config);

    match primary {
        Ok(p) => {
            if config.fallback.is_empty() {
                // Single provider, no chain needed
                info!(provider = config.primary.as_str(), "Provider chain: single provider");
                Some(p)
            } else {
                // Build chain: primary + fallbacks
                let mut providers = vec![p];
                for fb_id in &config.fallback {
                    match build_single_provider(fb_id, config) {
                        Ok(fb) => {
                            info!(provider = fb_id.as_str(), "Fallback provider added");
                            providers.push(fb);
                        }
                        Err(e) => {
                            warn!(provider = fb_id.as_str(), error = %e, "Fallback provider skipped");
                        }
                    }
                }
                let chain = FallbackChain::new(providers);
                info!(
                    chain = chain.provider_info().as_str(),
                    count = chain.len(),
                    "Provider chain built"
                );
                Some(Box::new(chain))
            }
        }
        Err(e) => {
            warn!(provider = config.primary.as_str(), error = %e, "Primary provider failed");

            // Try fallbacks as last resort
            for fb_id in &config.fallback {
                match build_single_provider(fb_id, config) {
                    Ok(fb) => {
                        info!(
                            provider = fb_id.as_str(),
                            "Fallback promoted to primary (original primary failed)"
                        );
                        return Some(fb);
                    }
                    Err(e) => {
                        warn!(provider = fb_id.as_str(), error = %e, "Fallback also failed");
                    }
                }
            }

            warn!("All providers failed — running in offline mode");
            None
        }
    }
}

/// Build a single provider instance, wrapped in ReliableProvider if retries > 0.
///
/// Resolves: model, API key, endpoint per provider ID.
fn build_single_provider(
    provider_id: &str,
    config: &ProviderConfigToml,
) -> Result<Box<dyn Provider>> {
    let api_key = resolve_api_key(provider_id, config);
    let model = resolve_model(provider_id, config);
    let endpoint = resolve_endpoint(provider_id, config);

    let provider_config = ProviderConfig {
        provider_id: provider_id.into(),
        endpoint,
        api_key,
        model,
        max_tokens: config.max_tokens,
        temperature: config.temperature,
    };

    let retries = config.max_retries;

    match provider_id {
        "anthropic" => {
            let p = crate::provider::anthropic::AnthropicProvider::new(provider_config)?;
            if retries > 0 {
                Ok(Box::new(ReliableProvider::new(p, retries)))
            } else {
                Ok(Box::new(p))
            }
        }
        "openai" => {
            let p = crate::provider::openai_compat::OpenAICompatibleProvider::openai(provider_config)?;
            if retries > 0 {
                Ok(Box::new(ReliableProvider::new(p, retries)))
            } else {
                Ok(Box::new(p))
            }
        }
        "deepseek" => {
            let p = crate::provider::openai_compat::OpenAICompatibleProvider::deepseek(provider_config)?;
            if retries > 0 {
                Ok(Box::new(ReliableProvider::new(p, retries)))
            } else {
                Ok(Box::new(p))
            }
        }
        "groq" => {
            let p = crate::provider::openai_compat::OpenAICompatibleProvider::groq(provider_config)?;
            if retries > 0 {
                Ok(Box::new(ReliableProvider::new(p, retries)))
            } else {
                Ok(Box::new(p))
            }
        }
        "ollama" => {
            let p = crate::provider::ollama::OllamaProvider::from_config(&provider_config)?;
            if retries > 0 {
                Ok(Box::new(ReliableProvider::new(p, retries)))
            } else {
                Ok(Box::new(p))
            }
        }
        "google" | "gemini" => {
            let p = crate::provider::gemini::GeminiProvider::new(provider_config)?;
            if retries > 0 {
                Ok(Box::new(ReliableProvider::new(p, retries)))
            } else {
                Ok(Box::new(p))
            }
        }
        other => Err(OneClawError::Provider(format!(
            "Unknown provider '{}'. Supported: anthropic, openai, deepseek, groq, ollama, google",
            other
        ))),
    }
}

/// Resolve API key for a specific provider.
///
/// Priority:
/// 1. Per-provider key from `[provider.keys]` table
/// 2. Global `api_key` from `[provider]`
/// 3. None (let provider constructor check env vars)
fn resolve_api_key(provider_id: &str, config: &ProviderConfigToml) -> Option<String> {
    // Normalize: "gemini" → "google" for key lookup
    let lookup_id = match provider_id {
        "gemini" => "google",
        other => other,
    };

    // 1. Per-provider key
    if let Some(key) = config.keys.get(lookup_id)
        && !key.is_empty()
    {
        return Some(key.clone());
    }

    // 2. Global api_key
    if let Some(key) = &config.api_key
        && !key.is_empty()
    {
        return Some(key.clone());
    }

    // 3. None — provider constructor will check env vars
    None
}

/// Resolve model name for a specific provider.
///
/// Logic:
/// - Primary provider uses config.model (or default if empty)
/// - Ollama: uses config.ollama_model if set, else config.model for primary, else default
/// - Fallback providers use their own default model
fn resolve_model(provider_id: &str, config: &ProviderConfigToml) -> String {
    // Special case: Ollama has its own model field
    if provider_id == "ollama"
        && let Some(ref model) = config.ollama_model
        && !model.is_empty()
    {
        return model.clone();
    }

    // Primary provider uses the configured model
    if (provider_id == config.primary || (provider_id == "gemini" && config.primary == "google"))
        && !config.model.is_empty()
    {
        return config.model.clone();
    }

    // Fallback: use the default model for this provider
    crate::provider::openai_compat::default_model_for_provider(provider_id).to_string()
}

/// Resolve endpoint for a specific provider.
///
/// Only Ollama supports custom endpoint from config.
/// Other providers use their built-in defaults.
fn resolve_endpoint(provider_id: &str, config: &ProviderConfigToml) -> Option<String> {
    if provider_id == "ollama" {
        return config.ollama_endpoint.clone();
    }
    None
}

/// Describe the chain for logging/status display.
///
/// Returns a human-readable string like:
///   "anthropic/claude-sonnet-4-20250514 (retries=1) → ollama/llama3.2:3b"
pub fn describe_chain(config: &ProviderConfigToml) -> String {
    let mut parts = vec![format!(
        "{}/{}",
        config.primary,
        if config.model.is_empty() {
            crate::provider::openai_compat::default_model_for_provider(&config.primary).to_string()
        } else {
            config.model.clone()
        }
    )];

    for fb in &config.fallback {
        let model = resolve_model(fb, config);
        parts.push(format!("{}/{}", fb, model));
    }

    if config.max_retries > 0 {
        format!("{} (retries={})", parts.join(" → "), config.max_retries)
    } else {
        parts.join(" → ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_api_key_per_provider() {
        let mut keys = std::collections::HashMap::new();
        keys.insert("openai".into(), "sk-per-provider".into());
        let config = ProviderConfigToml {
            api_key: Some("sk-global".into()),
            keys,
            ..Default::default()
        };
        assert_eq!(
            resolve_api_key("openai", &config),
            Some("sk-per-provider".into())
        );
    }

    #[test]
    fn test_resolve_api_key_global_fallback() {
        let config = ProviderConfigToml {
            api_key: Some("sk-global".into()),
            ..Default::default()
        };
        assert_eq!(
            resolve_api_key("deepseek", &config),
            Some("sk-global".into())
        );
    }

    #[test]
    fn test_resolve_api_key_none() {
        let config = ProviderConfigToml::default();
        assert_eq!(resolve_api_key("openai", &config), None);
    }

    #[test]
    fn test_resolve_api_key_gemini_alias() {
        let mut keys = std::collections::HashMap::new();
        keys.insert("google".into(), "AIza-key".into());
        let config = ProviderConfigToml {
            keys,
            ..Default::default()
        };
        // "gemini" should resolve to "google" key
        assert_eq!(
            resolve_api_key("gemini", &config),
            Some("AIza-key".into())
        );
    }

    #[test]
    fn test_resolve_model_primary() {
        let config = ProviderConfigToml {
            primary: "anthropic".into(),
            model: "claude-haiku-4-5-20251001".into(),
            ..Default::default()
        };
        assert_eq!(resolve_model("anthropic", &config), "claude-haiku-4-5-20251001");
    }

    #[test]
    fn test_resolve_model_fallback_uses_default() {
        let config = ProviderConfigToml {
            primary: "anthropic".into(),
            model: "claude-sonnet-4-20250514".into(),
            ..Default::default()
        };
        // Fallback "openai" should use its default, not the primary's model
        assert_eq!(resolve_model("openai", &config), "gpt-4o");
    }

    #[test]
    fn test_resolve_model_ollama_override() {
        let config = ProviderConfigToml {
            primary: "anthropic".into(),
            model: "claude-sonnet-4-20250514".into(),
            ollama_model: Some("qwen2.5:3b".into()),
            ..Default::default()
        };
        assert_eq!(resolve_model("ollama", &config), "qwen2.5:3b");
    }

    #[test]
    fn test_resolve_model_empty_uses_default() {
        let config = ProviderConfigToml {
            primary: "openai".into(),
            model: String::new(),
            ..Default::default()
        };
        assert_eq!(resolve_model("openai", &config), "gpt-4o");
    }

    #[test]
    fn test_resolve_endpoint_ollama() {
        let config = ProviderConfigToml {
            ollama_endpoint: Some("http://192.168.1.100:11434".into()),
            ..Default::default()
        };
        assert_eq!(
            resolve_endpoint("ollama", &config),
            Some("http://192.168.1.100:11434".into())
        );
    }

    #[test]
    fn test_resolve_endpoint_non_ollama() {
        let config = ProviderConfigToml::default();
        assert_eq!(resolve_endpoint("anthropic", &config), None);
        assert_eq!(resolve_endpoint("openai", &config), None);
    }

    #[test]
    fn test_build_provider_chain_single_anthropic() {
        let config = ProviderConfigToml {
            primary: "anthropic".into(),
            model: "claude-sonnet-4-20250514".into(),
            api_key: Some("sk-test-chain".into()),
            ..Default::default()
        };
        let chain = build_provider_chain(&config);
        assert!(chain.is_some());
        let p = chain.unwrap();
        // Single provider (not FallbackChain), id = "anthropic"
        assert_eq!(p.id(), "anthropic");
    }

    #[test]
    fn test_build_provider_chain_with_fallback() {
        let mut keys = std::collections::HashMap::new();
        keys.insert("openai".into(), "sk-openai-test".into());
        let config = ProviderConfigToml {
            primary: "anthropic".into(),
            model: "claude-sonnet-4-20250514".into(),
            api_key: Some("sk-test-chain".into()),
            fallback: vec!["openai".into()],
            keys,
            ..Default::default()
        };
        let chain = build_provider_chain(&config);
        assert!(chain.is_some());
        let p = chain.unwrap();
        // FallbackChain wraps multiple providers
        assert_eq!(p.id(), "fallback-chain");
    }

    #[test]
    fn test_build_provider_chain_no_key_graceful() {
        // No API key, no env vars → primary fails, no fallbacks → None
        let config = ProviderConfigToml {
            primary: "anthropic".into(),
            model: "claude-sonnet-4-20250514".into(),
            api_key: None,
            ..Default::default()
        };
        // This may succeed if ANTHROPIC_API_KEY is in env, so just verify no panic
        let _chain = build_provider_chain(&config);
    }

    #[test]
    fn test_build_provider_chain_unknown_primary_tries_fallback() {
        let config = ProviderConfigToml {
            primary: "nonexistent".into(),
            fallback: vec!["ollama".into()],
            ..Default::default()
        };
        let chain = build_provider_chain(&config);
        // Primary fails ("nonexistent"), ollama fallback should work
        assert!(chain.is_some());
        assert_eq!(chain.unwrap().id(), "ollama");
    }

    #[test]
    fn test_build_provider_chain_ollama_custom_endpoint() {
        let config = ProviderConfigToml {
            primary: "ollama".into(),
            ollama_endpoint: Some("http://192.168.1.50:11434".into()),
            ollama_model: Some("phi3:mini".into()),
            ..Default::default()
        };
        let chain = build_provider_chain(&config);
        assert!(chain.is_some());
        assert_eq!(chain.unwrap().id(), "ollama");
    }

    #[test]
    fn test_build_single_provider_with_retries() {
        let config = ProviderConfigToml {
            primary: "anthropic".into(),
            model: "claude-sonnet-4-20250514".into(),
            api_key: Some("sk-test".into()),
            max_retries: 2,
            ..Default::default()
        };
        let p = build_single_provider("anthropic", &config).unwrap();
        // Wrapped in ReliableProvider, but id() delegates to inner
        assert_eq!(p.id(), "anthropic");
    }

    #[test]
    fn test_build_single_provider_zero_retries() {
        let config = ProviderConfigToml {
            primary: "anthropic".into(),
            model: "claude-sonnet-4-20250514".into(),
            api_key: Some("sk-test".into()),
            max_retries: 0,
            ..Default::default()
        };
        let p = build_single_provider("anthropic", &config).unwrap();
        assert_eq!(p.id(), "anthropic");
    }

    #[test]
    fn test_describe_chain_single() {
        let config = ProviderConfigToml {
            primary: "anthropic".into(),
            model: "claude-sonnet-4-20250514".into(),
            max_retries: 1,
            ..Default::default()
        };
        let desc = describe_chain(&config);
        assert!(desc.contains("anthropic/claude-sonnet-4-20250514"));
        assert!(desc.contains("retries=1"));
    }

    #[test]
    fn test_describe_chain_with_fallback() {
        let config = ProviderConfigToml {
            primary: "anthropic".into(),
            model: "claude-sonnet-4-20250514".into(),
            fallback: vec!["openai".into(), "ollama".into()],
            max_retries: 2,
            ..Default::default()
        };
        let desc = describe_chain(&config);
        assert!(desc.contains("anthropic"));
        assert!(desc.contains("→ openai"));
        assert!(desc.contains("→ ollama"));
        assert!(desc.contains("retries=2"));
    }

    #[test]
    fn test_fallback_chain_len_and_is_empty() {
        let chain = FallbackChain::new(vec![
            Box::new(crate::provider::traits::NoopTestProvider::available()),
            Box::new(crate::provider::traits::NoopTestProvider::available()),
        ]);
        assert_eq!(chain.len(), 2);
        assert!(!chain.is_empty());

        let empty_chain = FallbackChain::new(vec![]);
        assert_eq!(empty_chain.len(), 0);
        assert!(empty_chain.is_empty());
    }

    #[test]
    fn test_fallback_chain_provider_info() {
        let chain = FallbackChain::new(vec![
            Box::new(crate::provider::traits::NoopTestProvider::available()),
            Box::new(crate::provider::traits::FailingTestProvider),
        ]);
        let info = chain.provider_info();
        assert!(info.contains("noop-test"));
        assert!(info.contains("failing-test"));
        assert!(info.contains("→"));

        let empty = FallbackChain::new(vec![]);
        assert_eq!(empty.provider_info(), "(empty)");
    }
}
