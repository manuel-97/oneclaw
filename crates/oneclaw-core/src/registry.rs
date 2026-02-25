//! Trait Registry — Central registry for all trait implementations
//!
//! The Registry is the "DI container" of OneClaw. Given a config,
//! it produces the correct trait implementations for each layer.
//! Runtime asks Registry what to use — Runtime never instantiates impls directly.

use crate::config::OneClawConfig;
use crate::error::Result;
use crate::security::{SecurityCore, DefaultSecurity};
use crate::orchestrator::router::{ModelRouter, NoopRouter};
use crate::orchestrator::context::ContextManager;
use crate::orchestrator::chain::ChainExecutor;
use crate::memory::{Memory, NoopMemory};
use crate::event_bus::EventBus;
use std::path::PathBuf;
use tracing::info;

/// Holds all resolved trait implementations.
pub struct ResolvedTraits {
    /// The resolved security core implementation.
    pub security: Box<dyn SecurityCore>,
    /// The resolved model router implementation.
    pub router: Box<dyn ModelRouter>,
    /// The resolved context manager implementation.
    pub context_mgr: Box<dyn ContextManager>,
    /// The resolved chain executor implementation.
    pub chain: Box<dyn ChainExecutor>,
    /// The resolved memory backend implementation.
    pub memory: Box<dyn Memory>,
    /// The resolved event bus implementation.
    pub event_bus: Box<dyn EventBus>,
    /// The resolved v1.5 provider (None = offline mode).
    pub provider: Option<Box<dyn crate::provider::Provider>>,
}

/// Trait Registry — resolves config to concrete implementations.
pub struct Registry;

impl Registry {
    /// Resolve all traits based on config.
    pub fn resolve(config: &OneClawConfig, workspace: impl Into<PathBuf>) -> Result<ResolvedTraits> {
        let workspace = workspace.into();

        // Layer 0: Security
        let security = Self::resolve_security(config, &workspace)?;

        // Layer 1: LLM Orchestrator (Noop until Sprint 5-6)
        let router = Self::resolve_router(config)?;
        let context_mgr = Self::resolve_context_manager(config)?;
        let chain = Self::resolve_chain(config)?;

        // Layer 2: Memory (Noop until Sprint 3-4)
        let memory = Self::resolve_memory(config)?;

        // Layer 3: Event Bus (Noop until Sprint 9-10)
        let event_bus = Self::resolve_event_bus(config)?;

        // v1.5 Provider (graceful: no key → None → offline mode)
        let provider = Self::resolve_provider(config);

        Ok(ResolvedTraits {
            security,
            router,
            context_mgr,
            chain,
            memory,
            event_bus,
            provider,
        })
    }

    fn resolve_security(config: &OneClawConfig, workspace: &PathBuf) -> Result<Box<dyn SecurityCore>> {
        let mut sec = if config.security.deny_by_default {
            info!("Security: DefaultSecurity (production)");
            DefaultSecurity::production(workspace)
        } else {
            info!("Security: DefaultSecurity (development)");
            DefaultSecurity::development(workspace)
        };

        // SQLite persistence (preferred, default)
        let mut sqlite_ok = false;
        if config.security.persist_pairing && !config.security.persist_path.is_empty() {
            match crate::security::SqliteSecurityStore::new(&config.security.persist_path) {
                Ok(store) => {
                    sec = sec.with_persistence(store);
                    info!(path = %config.security.persist_path, "Device pairing persistence enabled (SQLite)");
                    sqlite_ok = true;
                }
                Err(e) => {
                    tracing::warn!("Failed to open security DB: {}. Falling back to in-memory.", e);
                }
            }
        }

        // Legacy flat-file registry (backward compat — only if SQLite not active)
        if !sqlite_ok && !config.security.registry_path.is_empty() {
            sec = sec.with_registry_path(&config.security.registry_path);
            info!(path = %config.security.registry_path, "Device registry persistence enabled (legacy file)");
        }

        Ok(Box::new(sec))
    }

    fn resolve_router(config: &OneClawConfig) -> Result<Box<dyn ModelRouter>> {
        match config.providers.default.as_str() {
            "noop" | "" => {
                info!("Router: NoopRouter");
                Ok(Box::new(NoopRouter))
            }
            _ => {
                info!("Router: DefaultRouter (provider: {})", config.providers.default);
                Ok(Box::new(crate::orchestrator::router::DefaultRouter::from_config(&config.providers)))
            }
        }
    }

    fn resolve_context_manager(_config: &OneClawConfig) -> Result<Box<dyn ContextManager>> {
        info!("ContextManager: DefaultContextManager");
        Ok(Box::new(crate::orchestrator::context::DefaultContextManager::new(
            "You are OneClaw, a helpful AI assistant running on an edge device. Answer concisely and clearly."
        )))
    }

    fn resolve_chain(_config: &OneClawConfig) -> Result<Box<dyn ChainExecutor>> {
        info!("ChainExecutor: DefaultChainExecutor");
        Ok(Box::new(crate::orchestrator::chain::DefaultChainExecutor::new()))
    }

    fn resolve_memory(config: &OneClawConfig) -> Result<Box<dyn Memory>> {
        match config.memory.backend.as_str() {
            "sqlite" => {
                info!(path = %config.memory.db_path, "Memory: SqliteMemory");
                Ok(Box::new(crate::memory::SqliteMemory::new(&config.memory.db_path)?))
            }
            "noop" | "" => {
                info!("Memory: Noop");
                Ok(Box::new(NoopMemory::new()))
            }
            other => {
                info!("Memory: Noop (backend '{}' not yet implemented)", other);
                Ok(Box::new(NoopMemory::new()))
            }
        }
    }

    fn resolve_provider(config: &OneClawConfig) -> Option<Box<dyn crate::provider::Provider>> {
        let description = crate::provider::describe_chain(&config.provider);
        info!(chain = description.as_str(), "Building provider chain");

        match crate::provider::build_provider_chain(&config.provider) {
            Some(p) => {
                info!(
                    provider = p.id(),
                    chain = description.as_str(),
                    "v1.5 Provider chain ready"
                );
                Some(p)
            }
            None => {
                tracing::warn!("No providers available — running in offline mode");
                None
            }
        }
    }

    fn resolve_event_bus(_config: &OneClawConfig) -> Result<Box<dyn EventBus>> {
        info!("EventBus: DefaultEventBus");
        Ok(Box::new(crate::event_bus::DefaultEventBus::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_all_defaults() {
        let config = OneClawConfig::default_config();
        let workspace = std::env::current_dir().unwrap();
        let traits = Registry::resolve(&config, workspace).unwrap();
        // Security should be DefaultSecurity (deny_by_default=true)
        let action = crate::security::Action {
            kind: crate::security::ActionKind::Read,
            resource: "test".into(),
            actor: "unpaired".into(),
        };
        let permit = traits.security.authorize(&action).unwrap();
        assert!(!permit.granted); // deny-by-default, unpaired
    }

    #[test]
    fn test_resolve_development_mode() {
        let toml_str = r#"
[security]
deny_by_default = false

[runtime]
name = "dev"
"#;
        let config: OneClawConfig = toml::from_str(toml_str).unwrap();
        let workspace = std::env::current_dir().unwrap();
        let traits = Registry::resolve(&config, workspace).unwrap();
        let action = crate::security::Action {
            kind: crate::security::ActionKind::Execute,
            resource: "test".into(),
            actor: "any-device".into(),
        };
        let permit = traits.security.authorize(&action).unwrap();
        assert!(permit.granted);
    }

    #[test]
    fn test_resolve_unknown_provider_uses_default_router() {
        let toml_str = r#"
[providers]
default = "future-provider"
"#;
        let config: OneClawConfig = toml::from_str(toml_str).unwrap();
        let workspace = std::env::current_dir().unwrap();
        let traits = Registry::resolve(&config, workspace).unwrap();
        let choice = traits.router.route(
            crate::orchestrator::router::Complexity::Simple
        ).unwrap();
        // DefaultRouter maps unknown provider name through, noop model as fallback
        assert_eq!(choice.provider, "future-provider");
    }
}
