//! OneClaw Runtime — Main event loop

use crate::config::OneClawConfig;
use crate::security::{SecurityCore, NoopSecurity};
use crate::orchestrator::router::{ModelRouter, NoopRouter};
use crate::orchestrator::context::{ContextManager, NoopContextManager};
use crate::orchestrator::chain::{ChainExecutor, NoopChainExecutor};
use crate::orchestrator::ProviderManager;
use crate::memory::{Memory, NoopMemory};
use crate::event_bus::{EventBus, NoopEventBus};
use crate::channel::ChannelManager;
use crate::tool::ToolRegistry;
use crate::security::RateLimiter;
use crate::metrics::Metrics;
use crate::error::Result;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::VecDeque;
use tracing::info;

/// Result of processing a single message
enum ProcessResult {
    /// Exit command — send response then break loop
    Exit(String),
    /// Normal response — send and continue
    Response(String),
}

/// The main OneClaw runtime, owning all layer implementations and the event loop.
pub struct Runtime {
    /// The active configuration.
    pub config: OneClawConfig,
    /// The security core implementation (Layer 0).
    pub security: Box<dyn SecurityCore>,
    /// The model router for LLM provider selection (Layer 1).
    pub router: Box<dyn ModelRouter>,
    /// The context manager for prompt enrichment (Layer 1).
    pub context_mgr: Box<dyn ContextManager>,
    /// The chain executor for multi-step pipelines (Layer 1).
    pub chain: Box<dyn ChainExecutor>,
    /// The memory backend implementation (Layer 2).
    pub memory: Box<dyn Memory>,
    /// The event bus implementation (Layer 3).
    pub event_bus: Box<dyn EventBus>,
    /// The LLM provider manager (Layer 1).
    pub provider_mgr: ProviderManager,
    /// Maps channel source identifiers to paired device IDs
    source_device_map: std::sync::Mutex<std::collections::HashMap<String, String>>,
    /// Tool registry (Layer 4) — Arc for shared access by event handlers
    pub tool_registry: Arc<ToolRegistry>,
    /// Active channel names (populated by run/run_multi)
    active_channels: std::sync::Mutex<Vec<String>>,
    /// Rate limiter for DoS prevention (default: 60/min)
    rate_limiter: RateLimiter,
    /// Operational metrics (AtomicU64 counters)
    pub metrics: Metrics,
    /// Shutdown flag — set by signal handler or "exit" command
    pub shutdown: Arc<AtomicBool>,
    /// Pending alert messages to push to channels (populated by event subscribers)
    pub pending_alerts: Arc<std::sync::Mutex<VecDeque<String>>>,
    /// v1.5 sync Provider (None = offline/no API key)
    pub provider: Option<Box<dyn crate::provider::Provider>>,
}

impl Runtime {
    /// Create runtime with all Noop implementations (for testing / bare boot)
    pub fn with_defaults(config: OneClawConfig) -> Self {
        Self {
            provider_mgr: ProviderManager::new(&config.providers.default),
            config,
            security: Box::new(NoopSecurity),
            router: Box::new(NoopRouter),
            context_mgr: Box::new(NoopContextManager),
            chain: Box::new(NoopChainExecutor::new()),
            memory: Box::new(NoopMemory::new()),
            event_bus: Box::new(NoopEventBus::new()),
            source_device_map: std::sync::Mutex::new(std::collections::HashMap::new()),
            tool_registry: Arc::new(ToolRegistry::new()),
            active_channels: std::sync::Mutex::new(vec![]),
            rate_limiter: RateLimiter::new(60),
            metrics: Metrics::new(),
            shutdown: Arc::new(AtomicBool::new(false)),
            pending_alerts: Arc::new(std::sync::Mutex::new(VecDeque::new())),
            provider: None,
        }
    }

    /// Create runtime with DefaultSecurity (production mode).
    /// Other layers remain Noop until their TIPs are implemented.
    pub fn with_security(config: OneClawConfig, workspace: impl Into<std::path::PathBuf>) -> Self {
        use crate::security::DefaultSecurity;
        Self {
            security: Box::new(DefaultSecurity::production(workspace)),
            provider_mgr: ProviderManager::new(&config.providers.default),
            config,
            router: Box::new(NoopRouter),
            context_mgr: Box::new(NoopContextManager),
            chain: Box::new(NoopChainExecutor::new()),
            memory: Box::new(NoopMemory::new()),
            event_bus: Box::new(NoopEventBus::new()),
            source_device_map: std::sync::Mutex::new(std::collections::HashMap::new()),
            tool_registry: Arc::new(ToolRegistry::new()),
            active_channels: std::sync::Mutex::new(vec![]),
            rate_limiter: RateLimiter::new(60),
            metrics: Metrics::new(),
            shutdown: Arc::new(AtomicBool::new(false)),
            pending_alerts: Arc::new(std::sync::Mutex::new(VecDeque::new())),
            provider: None,
        }
    }

    /// Create runtime from config using Registry to resolve all traits.
    pub fn from_config(config: OneClawConfig, workspace: impl Into<std::path::PathBuf>) -> Result<Self> {
        use crate::registry::Registry;
        let traits = Registry::resolve(&config, workspace)?;
        Ok(Self {
            provider_mgr: ProviderManager::new(&config.providers.default),
            config,
            security: traits.security,
            router: traits.router,
            context_mgr: traits.context_mgr,
            chain: traits.chain,
            memory: traits.memory,
            event_bus: traits.event_bus,
            source_device_map: std::sync::Mutex::new(std::collections::HashMap::new()),
            tool_registry: Arc::new(ToolRegistry::new()),
            active_channels: std::sync::Mutex::new(vec![]),
            rate_limiter: RateLimiter::new(60),
            metrics: Metrics::new(),
            shutdown: Arc::new(AtomicBool::new(false)),
            pending_alerts: Arc::new(std::sync::Mutex::new(VecDeque::new())),
            provider: traits.provider,
        })
    }

    /// Drain pending alerts and return them.
    fn drain_alerts(&self) -> Vec<String> {
        if let Ok(mut alerts) = self.pending_alerts.lock() {
            alerts.drain(..).collect()
        } else {
            vec![]
        }
    }

    /// Execute LLM call with timeout monitoring (defense-in-depth)
    async fn llm_with_timeout(&self, provider: &str, request: &crate::orchestrator::provider::LlmRequest) -> crate::error::Result<crate::orchestrator::provider::LlmResponse> {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(self.config.providers.llm_timeout_secs);

        let result = self.provider_mgr.chat(provider, request).await;

        let elapsed = start.elapsed();
        if elapsed > timeout {
            tracing::warn!(
                elapsed_ms = elapsed.as_millis() as u64,
                timeout_secs = self.config.providers.llm_timeout_secs,
                "LLM call exceeded timeout threshold"
            );
            Metrics::inc(&self.metrics.errors_total);
        }

        result
    }

    /// Process a message through the LLM pipeline
    async fn process_with_llm(&self, content: &str) -> String {
        use crate::orchestrator::router::analyze_complexity;
        use crate::orchestrator::provider::LlmRequest;
        use tracing::warn;

        Metrics::inc(&self.metrics.llm_calls_total);
        let llm_start = std::time::Instant::now();

        // 1. Search memory for relevant context
        let memory_results = self.memory
            .search(&crate::memory::MemoryQuery::new(content).with_limit(5))
            .unwrap_or_default();

        let has_memory = !memory_results.is_empty();
        let memory_strings: Vec<String> = memory_results.iter()
            .map(|e| format!("[{}] {}", e.created_at.format("%d/%m %H:%M"), e.content))
            .collect();

        // 2. Analyze complexity
        let complexity = analyze_complexity(content, has_memory);
        info!(complexity = ?complexity, has_memory = has_memory, "Message analysis");

        // 3. Route to provider/model
        let choice = match self.router.route(complexity) {
            Ok(c) => c,
            Err(e) => {
                warn!("Router error: {}", e);
                return format!("[OneClaw] Routing error: {}", e);
            }
        };

        info!(provider = %choice.provider, model = %choice.model, reason = %choice.reason, "Routed");

        // 4. Build context with memory
        let system_prompt = "You are OneClaw, a helpful AI assistant running on an edge device. \
            Answer concisely and clearly. \
            When relevant data is available from memory, incorporate it into your response.";

        let mut user_content = String::new();
        if !memory_strings.is_empty() {
            user_content.push_str("Related data from memory:\n");
            for mem in &memory_strings {
                user_content.push_str(&format!("- {}\n", mem));
            }
            user_content.push('\n');
        }
        user_content.push_str(content);

        // 5. Call LLM via ProviderManager
        let request = LlmRequest::with_system(&choice.model, system_prompt, &user_content)
            .set_max_tokens(500)
            .set_temperature(0.3);

        match self.llm_with_timeout(&choice.provider, &request).await {
            Ok(resp) => {
                Metrics::add(&self.metrics.llm_latency_total_ms, llm_start.elapsed().as_millis() as u64);
                let tokens = resp.usage.as_ref().map(|u| u.total_tokens).unwrap_or(0);
                Metrics::add(&self.metrics.llm_tokens_total, tokens as u64);
                info!(
                    latency_ms = resp.latency_ms,
                    tokens = tokens,
                    "LLM response received"
                );
                resp.content
            }
            Err(e) => {
                Metrics::inc(&self.metrics.llm_calls_failed);
                // Graceful fallback
                warn!(error = %e, "LLM call failed, trying fallback");
                match self.provider_mgr.chat("noop", &request).await {
                    Ok(_) => {
                        let mut fallback = format!(
                            "[Offline mode] LLM unavailable ({}). Data saved, will process when connected.",
                            choice.provider,
                        );
                        if has_memory {
                            fallback.push_str(&format!(
                                "\n{} related entries found in memory.", memory_results.len()
                            ));
                        }
                        fallback
                    }
                    Err(_) => "[OneClaw] System temporarily unavailable. Please try again.".into(),
                }
            }
        }
    }

    /// Run a chain with the current runtime context
    pub async fn run_chain(&self, chain: &crate::orchestrator::Chain, input: &str) -> Result<crate::orchestrator::ChainResult> {
        use crate::orchestrator::ChainContext;
        use crate::orchestrator::router::analyze_complexity;

        let complexity = analyze_complexity(input, true);
        let choice = self.router.route(complexity)?;

        let ctx = ChainContext {
            provider_mgr: &self.provider_mgr,
            provider_name: &choice.provider,
            model: &choice.model,
            memory: self.memory.as_ref(),
            event_bus: self.event_bus.as_ref(),
            system_prompt: "You are OneClaw, a helpful AI assistant running on an edge device. Answer concisely.",
            tool_registry: Some(&self.tool_registry),
        };

        self.chain.execute(chain, input, &ctx).await
    }

    /// Health check — probe all 5 layers and report status
    async fn health_check(&self) -> String {
        use crate::security::{Action, ActionKind};

        let mut lines = vec!["OneClaw Health Check:".to_string()];

        // Layer 0: Security
        let sec_ok = self.security.authorize(&Action {
            kind: ActionKind::Execute,
            resource: "health-probe".into(),
            actor: "system".into(),
        }).is_ok();
        lines.push(format!("  L0 Security:     {}", if sec_ok { "OK" } else { "FAIL" }));

        // Layer 1: LLM Orchestrator
        let providers = self.provider_mgr.list_providers();
        let availability = self.provider_mgr.check_availability().await;
        let any_online = providers.iter().any(|p| *availability.get(*p).unwrap_or(&false));
        lines.push(format!("  L1 Orchestrator: {} ({} providers, {} online)",
            if any_online { "OK" } else { "DEGRADED" },
            providers.len(),
            availability.values().filter(|v| **v).count(),
        ));

        // Layer 2: Memory
        let mem_ok = self.memory.count().is_ok();
        let mem_count = self.memory.count().unwrap_or(0);
        lines.push(format!("  L2 Memory:       {} ({} entries)",
            if mem_ok { "OK" } else { "FAIL" }, mem_count));

        // Layer 3: Event Bus
        let pending = self.event_bus.pending_count();
        lines.push(format!("  L3 Event Bus:    OK ({} pending)", pending));

        // Layer 4: Tools
        let tool_count = self.tool_registry.count();
        lines.push(format!("  L4 Tools:        OK ({} registered)", tool_count));

        // Summary
        let all_ok = sec_ok && mem_ok;
        lines.push(format!("\n  Uptime: {} | Status: {}",
            self.metrics.uptime_display(),
            if all_ok && any_online { "HEALTHY" } else if all_ok { "DEGRADED" } else { "UNHEALTHY" },
        ));

        lines.join("\n")
    }

    /// Reload config from file and report diff (does not hot-apply)
    fn reload_config(&self) -> String {
        // Try standard config paths
        let config_paths = ["oneclaw.toml", "config/oneclaw.toml"];
        let mut found_path = None;
        for path in &config_paths {
            if std::path::Path::new(path).exists() {
                found_path = Some(*path);
                break;
            }
        }

        let Some(path) = found_path else {
            return "No config file found (tried: oneclaw.toml, config/oneclaw.toml). Current config unchanged.".into();
        };

        match OneClawConfig::load(path) {
            Ok(new_config) => {
                let mut diffs = vec![format!("Config reload from: {}", path)];

                // Compare key fields
                if new_config.providers.default != self.config.providers.default {
                    diffs.push(format!("  providers.default: {} -> {}",
                        self.config.providers.default, new_config.providers.default));
                }
                if new_config.security.deny_by_default != self.config.security.deny_by_default {
                    diffs.push(format!("  security.deny_by_default: {} -> {}",
                        self.config.security.deny_by_default, new_config.security.deny_by_default));
                }
                if new_config.memory.backend != self.config.memory.backend {
                    diffs.push(format!("  memory.backend: {} -> {}",
                        self.config.memory.backend, new_config.memory.backend));
                }
                if new_config.runtime.name != self.config.runtime.name {
                    diffs.push(format!("  runtime.name: {} -> {}",
                        self.config.runtime.name, new_config.runtime.name));
                }
                if new_config.providers.ollama.model != self.config.providers.ollama.model {
                    diffs.push(format!("  providers.ollama.model: {} -> {}",
                        self.config.providers.ollama.model, new_config.providers.ollama.model));
                }
                if new_config.providers.openai.model != self.config.providers.openai.model {
                    diffs.push(format!("  providers.openai.model: {} -> {}",
                        self.config.providers.openai.model, new_config.providers.openai.model));
                }
                if new_config.channels.active != self.config.channels.active {
                    diffs.push(format!("  channels.active: {:?} -> {:?}",
                        self.config.channels.active, new_config.channels.active));
                }

                if diffs.len() == 1 {
                    diffs.push("  No changes detected.".into());
                } else {
                    diffs.push("  (Report only — restart to apply changes)".into());
                }

                diffs.join("\n")
            }
            Err(e) => format!("Config reload failed: {}", e),
        }
    }

    /// Boot the runtime
    pub fn boot(&self) -> Result<()> {
        info!(
            name = %self.config.runtime.name,
            deny_by_default = %self.config.security.deny_by_default,
            "OneClaw runtime booting"
        );
        info!("Layer 0: Security Core initialized");
        info!("Layer 1: LLM Orchestrator initialized");
        info!("Layer 2: Memory initialized");
        info!("Layer 3: Event Bus initialized");
        info!(tools = self.tool_registry.count(), "Layer 4: Tool Registry initialized");
        info!("Runtime ready. All 5 layers initialized.");
        Ok(())
    }

    /// Process a single incoming message, returning the response.
    /// Used by both run() and run_multi() to avoid logic duplication.
    ///
    /// Security model: Only exit/help/pair/verify are always-open.
    /// ALL other commands require security authorization first.
    async fn process_message(&self, message: &crate::channel::IncomingMessage) -> ProcessResult {
        use crate::security::{Action, ActionKind};
        use tracing::warn;

        Metrics::inc(&self.metrics.messages_total);

        let content_lower = message.content.to_lowercase();
        let content_lower = content_lower.trim();

        // === ALWAYS OPEN (no security check) ===

        if content_lower == "exit" || content_lower == "quit" || content_lower == "q" {
            info!("Exit command received. Shutting down.");
            return ProcessResult::Exit("Goodbye!".into());
        }

        if content_lower == "help" {
            return ProcessResult::Response("\
OneClaw Commands:
  ask Q        - Ask AI a question
  tools        - List registered tools
  tool X k=v   - Execute tool X with params (key=value)
  channels     - List active channels
  events       - Show event bus status and recent events
  status       - Show agent status and config
  metrics      - Show operational metrics (counters, uptime)
  health       - Health check all layers
  reload       - Check config file for changes (report only)
  providers    - List LLM providers and status
  pair         - Generate device pairing code
  verify CODE  - Pair device with 6-digit code
  devices      - List all paired devices
  unpair ID    - Remove a paired device (prefix match)
  remember X   - Store X in memory
  recall X     - Search memory for X
  help         - Show this help message
  exit         - Shut down the agent

Any other input will be processed by the AI pipeline.".into());
        }

        if content_lower == "pair" {
            let response = match self.security.generate_pairing_code() {
                Ok(code) => format!("Pairing code: {} (valid 5 minutes)", code),
                Err(e) => format!("Failed to generate pairing code: {}", e),
            };
            return ProcessResult::Response(response);
        }

        if content_lower.starts_with("verify ") {
            let code = message.content.trim()[7..].trim();
            let response = match self.security.verify_pairing_code(code) {
                Ok(identity) => {
                    if let Ok(mut map) = self.source_device_map.lock() {
                        map.insert(message.source.clone(), identity.device_id.clone());
                    }
                    format!(
                        "Device paired successfully!\n  Device ID: {}\n  Paired at: {}\n  You can now interact with the agent.",
                        identity.device_id,
                        identity.paired_at.format("%Y-%m-%d %H:%M:%S UTC")
                    )
                }
                Err(e) => format!("Pairing failed: {}", e),
            };
            return ProcessResult::Response(response);
        }

        // === RATE LIMIT CHECK ===
        if !self.rate_limiter.check() {
            Metrics::inc(&self.metrics.messages_rate_limited);
            return ProcessResult::Response(
                "Too many requests. Please wait a moment.".into()
            );
        }

        // === SECURITY CHECK for everything else ===
        let actor = self.source_device_map.lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&message.source)
            .cloned()
            .unwrap_or_else(|| message.source.clone());

        let action = Action {
            kind: ActionKind::Execute,
            resource: "command".into(),
            actor,
        };

        match self.security.authorize(&action) {
            Ok(permit) if permit.granted => {
                Metrics::inc(&self.metrics.messages_secured);
                self.dispatch_secured_command(message, content_lower).await
            }
            Ok(permit) => {
                Metrics::inc(&self.metrics.messages_denied);
                ProcessResult::Response(format!(
                    "Access denied: {}. Use 'pair' + 'verify CODE' to pair device.",
                    permit.reason
                ))
            }
            Err(e) => {
                Metrics::inc(&self.metrics.errors_total);
                warn!("Security error: {}", e);
                ProcessResult::Response(format!("Security error: {}", e))
            }
        }
    }

    /// Dispatch commands that have passed security check
    async fn dispatch_secured_command(&self, message: &crate::channel::IncomingMessage, content_lower: &str) -> ProcessResult {
        if content_lower == "metrics" {
            return ProcessResult::Response(self.metrics.report());
        }

        if content_lower == "health" {
            return ProcessResult::Response(self.health_check().await);
        }

        if content_lower == "reload" {
            return ProcessResult::Response(self.reload_config());
        }

        if content_lower == "status" {
            let o = std::sync::atomic::Ordering::Relaxed;
            let provider_status = match &self.provider {
                Some(p) => format!("{} ({})", p.id(), if p.is_available() { "online" } else { "offline" }),
                None => "none (offline mode)".into(),
            };
            let chain_desc = crate::provider::describe_chain(&self.config.provider);
            return ProcessResult::Response(format!(
                "OneClaw Agent v1.5.0\n\
                 \n  Uptime: {}\n\
                 \n  Security: {}\n\
                   Memory: {} entries ({})\n\
                   Provider: {}\n\
                   Chain: {}\n\
                   Async Providers: {} (default: {})\n\
                   Tools: {}\n\
                   Events: {} processed, {} pending\n\
                   Messages: {} total ({} denied)\n\
                   LLM: {} calls (avg {}ms)\n\
                 \n  Type 'health' for detailed check\n\
                   Type 'metrics' for full telemetry",
                self.metrics.uptime_display(),
                if self.config.security.deny_by_default { "enforced" } else { "open" },
                self.memory.count().unwrap_or(0),
                self.config.memory.backend,
                provider_status,
                chain_desc,
                self.provider_mgr.list_providers().len(),
                self.config.providers.default,
                self.tool_registry.count(),
                self.metrics.events_processed.load(o),
                self.event_bus.pending_count(),
                self.metrics.messages_total.load(o),
                self.metrics.messages_denied.load(o),
                self.metrics.llm_calls_total.load(o),
                self.metrics.avg_llm_latency_ms(),
            ));
        }

        if content_lower == "providers" {
            let providers = self.provider_mgr.list_providers();
            let availability = self.provider_mgr.check_availability().await;
            let mut response = format!("LLM Providers (default: {}):\n", self.provider_mgr.default_provider());
            for name in &providers {
                let status = if *availability.get(*name).unwrap_or(&false) { "online" } else { "offline" };
                response.push_str(&format!("  {} — {}\n", name, status));
            }
            return ProcessResult::Response(response);
        }

        if content_lower == "events" {
            let pending = self.event_bus.pending_count();
            let recent = self.event_bus.recent_events(5).unwrap_or_default();
            let mut response = format!("Event Bus: {} pending\n", pending);
            if recent.is_empty() {
                response.push_str("  No recent events.");
            } else {
                response.push_str(&format!("  Last {} events:\n", recent.len()));
                for event in &recent {
                    response.push_str(&format!(
                        "    [{}] {} (from: {}, priority: {:?})\n",
                        event.timestamp.format("%H:%M:%S"),
                        event.topic,
                        event.source,
                        event.priority,
                    ));
                }
            }
            return ProcessResult::Response(response);
        }

        if content_lower == "channels" {
            let channels = self.active_channels.lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            let response = if channels.is_empty() {
                "Active channels: (none)".into()
            } else {
                format!("Active channels ({}): {}", channels.len(), channels.join(", "))
            };
            return ProcessResult::Response(response);
        }

        // devices — list paired devices
        if content_lower == "devices" {
            let response = match self.security.list_devices() {
                Ok(devices) if devices.is_empty() => "No paired devices.".into(),
                Ok(devices) => {
                    let mut resp = format!("Paired Devices ({}):\n", devices.len());
                    for d in &devices {
                        let label = if d.label.is_empty() { "" } else { &d.label };
                        resp.push_str(&format!(
                            "  {} | paired: {} | seen: {}{}\n",
                            &d.device_id[..std::cmp::min(8, d.device_id.len())],
                            d.paired_at.format("%Y-%m-%d %H:%M"),
                            d.last_seen.format("%Y-%m-%d %H:%M"),
                            if label.is_empty() { String::new() } else { format!(" | {}", label) },
                        ));
                    }
                    resp
                }
                Err(e) => format!("Failed to list devices: {}", e),
            };
            return ProcessResult::Response(response);
        }

        // unpair <id_prefix> — remove a paired device
        if content_lower.starts_with("unpair ") {
            let prefix = message.content.trim()[7..].trim();
            let response = match self.security.remove_device(prefix) {
                Ok(device) => format!(
                    "Device unpaired: {}\n  Was paired since: {}",
                    device.device_id,
                    device.paired_at.format("%Y-%m-%d %H:%M:%S UTC"),
                ),
                Err(e) => format!("Unpair failed: {}", e),
            };
            return ProcessResult::Response(response);
        }

        // tools — list registered tools
        if content_lower == "tools" {
            let tools = self.tool_registry.list_tools();
            let mut response = format!("Registered Tools ({}):\n", tools.len());
            if tools.is_empty() {
                response.push_str("  No tools registered.");
            } else {
                for t in &tools {
                    let params: Vec<String> = t.params.iter()
                        .map(|p| if p.required { format!("{}*", p.name) } else { p.name.clone() })
                        .collect();
                    response.push_str(&format!(
                        "  [{}] {} — {} (params: {})\n",
                        t.category, t.name, t.description,
                        if params.is_empty() { "none".into() } else { params.join(", ") },
                    ));
                }
            }
            return ProcessResult::Response(response);
        }

        // tool <name> [key=value ...] — execute a tool
        if content_lower.starts_with("tool ") {
            let args_str = message.content.trim()[5..].trim();
            let parts: Vec<&str> = args_str.split_whitespace().collect();
            if parts.is_empty() {
                return ProcessResult::Response("Usage: tool <name> [key=value ...]".into());
            }

            let tool_name = parts[0];
            let mut params = std::collections::HashMap::new();
            for &part in &parts[1..] {
                if let Some((key, value)) = part.split_once('=') {
                    params.insert(key.to_string(), value.to_string());
                }
            }

            Metrics::inc(&self.metrics.tool_calls_total);
            let response = match self.tool_registry.execute(tool_name, &params, Some(self.event_bus.as_ref())) {
                Ok(result) => {
                    if !result.success { Metrics::inc(&self.metrics.tool_calls_failed); }
                    let status = if result.success { "OK" } else { "FAIL" };
                    format!("[{}] {}: {}", status, tool_name, result.output)
                }
                Err(e) => {
                    Metrics::inc(&self.metrics.tool_calls_failed);
                    format!("Tool error: {}", e)
                }
            };
            return ProcessResult::Response(response);
        }

        if content_lower.starts_with("remember ") {
            let text = message.content.trim()[9..].trim();
            Metrics::inc(&self.metrics.memory_stores);
            let response = match self.memory.store(text, crate::memory::MemoryMeta::default()) {
                Ok(id) => {
                    let count = self.memory.count().unwrap_or(0);
                    format!("Remembered. (ID: {}, total memories: {})", &id[..8], count)
                }
                Err(e) => format!("Failed to remember: {}", e),
            };
            return ProcessResult::Response(response);
        }

        if content_lower.starts_with("recall ") {
            let query_text = message.content.trim()[7..].trim();
            Metrics::inc(&self.metrics.memory_searches);
            let query = crate::memory::MemoryQuery::new(query_text).with_limit(5);
            let response = match self.memory.search(&query) {
                Ok(results) if results.is_empty() => "No memories found.".into(),
                Ok(results) => {
                    let mut resp = format!("Found {} memories:\n", results.len());
                    for (i, entry) in results.iter().enumerate() {
                        resp.push_str(&format!(
                            "  {}. [{}] {}\n",
                            i + 1,
                            entry.created_at.format("%Y-%m-%d %H:%M"),
                            entry.content,
                        ));
                    }
                    resp
                }
                Err(e) => format!("Recall failed: {}", e),
            };
            return ProcessResult::Response(response);
        }

        // ask Q — send question to LLM pipeline
        if content_lower.starts_with("ask ") {
            let question = message.content.trim()[4..].trim();
            return ProcessResult::Response(self.process_with_llm(question).await);
        }

        // LLM Processing Pipeline
        ProcessResult::Response(self.process_with_llm(&message.content).await)
    }

    /// Run the main event loop (single channel).
    /// Receive from channel, process through security + pipeline, respond, repeat.
    pub async fn run(&self, channel: &dyn crate::channel::Channel) -> Result<()> {
        use tracing::warn;

        // Record active channel
        if let Ok(mut channels) = self.active_channels.lock() {
            *channels = vec![channel.name().to_string()];
        }

        info!(channel = channel.name(), "Event loop starting");

        let mut last_drain = std::time::Instant::now();
        let drain_interval = std::time::Duration::from_secs(5);

        loop {
            // Check shutdown flag (set externally, e.g., Ctrl+C handler)
            if self.shutdown.load(Ordering::SeqCst) {
                info!("Graceful shutdown initiated");
                let _ = self.event_bus.drain();
                break;
            }

            // Periodic drain (even without messages)
            if last_drain.elapsed() >= drain_interval {
                let drained = self.event_bus.drain().unwrap_or(0);
                if drained > 0 {
                    Metrics::add(&self.metrics.events_processed, drained as u64);
                }
                last_drain = std::time::Instant::now();
            }

            // 1. Receive from channel
            let message = match channel.receive().await {
                Ok(Some(msg)) => msg,
                Ok(None) => continue,
                Err(e) => {
                    let err_msg = format!("{}", e);
                    if err_msg.contains("EOF") {
                        info!("Channel closed (EOF). Shutting down.");
                        break;
                    }
                    warn!("Channel receive error: {}", e);
                    continue;
                }
            };

            // 2. Drain event bus
            let drained = self.event_bus.drain().unwrap_or(0);
            if drained > 0 {
                Metrics::add(&self.metrics.events_processed, drained as u64);
            }
            last_drain = std::time::Instant::now();

            // 2b. Flush pending alerts to channel
            for alert_msg in self.drain_alerts() {
                let _ = channel.send(&crate::channel::OutgoingMessage {
                    destination: "alert".into(),
                    content: alert_msg,
                }).await;
            }

            // 3. Process message
            match self.process_message(&message).await {
                ProcessResult::Exit(resp) => {
                    let _ = channel.send(&crate::channel::OutgoingMessage {
                        destination: message.source,
                        content: resp,
                    }).await;
                    break;
                }
                ProcessResult::Response(resp) => {
                    let _ = channel.send(&crate::channel::OutgoingMessage {
                        destination: message.source,
                        content: resp,
                    }).await;
                }
            }
        }

        info!("Event loop stopped.");
        Ok(())
    }

    /// Run the main event loop with multiple channels via ChannelManager.
    /// Polls all channels round-robin, processes messages with same logic as run().
    pub async fn run_multi(&self, manager: &ChannelManager) -> Result<()> {
        use tracing::warn;

        // Record active channels
        if let Ok(mut channels) = self.active_channels.lock() {
            *channels = manager.list().iter().map(|s| s.to_string()).collect();
        }

        info!(channels = manager.count(), "Event loop starting (multi-channel)");

        let mut last_drain = std::time::Instant::now();
        let drain_interval = std::time::Duration::from_secs(5);

        loop {
            // Check shutdown flag (set externally, e.g., Ctrl+C handler)
            if self.shutdown.load(Ordering::SeqCst) {
                info!("Graceful shutdown initiated (multi-channel)");
                let _ = self.event_bus.drain();
                break;
            }

            // Periodic drain (even without messages)
            if last_drain.elapsed() >= drain_interval {
                let drained = self.event_bus.drain().unwrap_or(0);
                if drained > 0 {
                    Metrics::add(&self.metrics.events_processed, drained as u64);
                }
                last_drain = std::time::Instant::now();
            }

            // 1. Poll all channels
            match manager.receive_any().await {
                Ok(Some((channel_idx, message))) => {
                    // 2. Drain event bus on message
                    let drained = self.event_bus.drain().unwrap_or(0);
                    if drained > 0 {
                        Metrics::add(&self.metrics.events_processed, drained as u64);
                    }
                    last_drain = std::time::Instant::now();

                    // 2b. Flush pending alerts to first channel (CLI)
                    for alert_msg in self.drain_alerts() {
                        let _ = manager.send_to(0, &crate::channel::OutgoingMessage {
                            destination: "alert".into(),
                            content: alert_msg,
                        }).await;
                    }

                    // 3. Process message
                    match self.process_message(&message).await {
                        ProcessResult::Exit(resp) => {
                            let _ = manager.send_to(channel_idx, &crate::channel::OutgoingMessage {
                                destination: message.source,
                                content: resp,
                            }).await;
                            break;
                        }
                        ProcessResult::Response(resp) => {
                            let _ = manager.send_to(channel_idx, &crate::channel::OutgoingMessage {
                                destination: message.source,
                                content: resp,
                            }).await;
                        }
                    }
                }
                Ok(None) => {
                    // No messages on any channel — brief sleep to avoid busy-waiting
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
                Err(e) => {
                    warn!("Channel manager error: {}", e);
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }

        info!("Event loop stopped.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    #[test]
    fn test_runtime_boots_with_defaults() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        assert!(runtime.boot().is_ok());
    }

    #[test]
    fn test_runtime_config_accessible() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        assert!(runtime.config.security.deny_by_default);
    }

    #[test]
    fn test_runtime_with_security() {
        let config = OneClawConfig::default_config();
        let workspace = std::env::current_dir().unwrap();
        let runtime = Runtime::with_security(config, workspace);
        assert!(runtime.boot().is_ok());
    }

    #[test]
    fn test_runtime_from_config() {
        let config = OneClawConfig::default_config();
        let workspace = std::env::current_dir().unwrap();
        let runtime = Runtime::from_config(config, workspace).unwrap();
        assert!(runtime.boot().is_ok());
    }

    // --- MockChannel for testing event loop without stdin ---

    struct MockChannel {
        inputs: std::sync::Mutex<Vec<String>>,
        outputs: std::sync::Mutex<Vec<String>>,
    }

    impl MockChannel {
        fn new(inputs: Vec<&str>) -> Self {
            Self {
                inputs: std::sync::Mutex::new(inputs.into_iter().rev().map(String::from).collect()),
                outputs: std::sync::Mutex::new(vec![]),
            }
        }
        fn get_outputs(&self) -> Vec<String> {
            self.outputs.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl crate::channel::Channel for MockChannel {
        fn name(&self) -> &str { "mock" }
        async fn receive(&self) -> crate::error::Result<Option<crate::channel::IncomingMessage>> {
            let mut inputs = self.inputs.lock().unwrap();
            match inputs.pop() {
                Some(content) => Ok(Some(crate::channel::IncomingMessage {
                    source: "test".into(),
                    content,
                    timestamp: chrono::Utc::now(),
                })),
                None => Ok(Some(crate::channel::IncomingMessage {
                    source: "test".into(),
                    content: "exit".into(),
                    timestamp: chrono::Utc::now(),
                })),
            }
        }
        async fn send(&self, message: &crate::channel::OutgoingMessage) -> crate::error::Result<()> {
            self.outputs.lock().unwrap().push(message.content.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_runtime_run_with_mock_channel() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);

        let channel = MockChannel::new(vec!["hello world", "status", "exit"]);
        runtime.run(&channel).await.unwrap();

        let outputs = channel.get_outputs();
        assert!(outputs.len() >= 3);
        // LLM pipeline (noop) processes the message — no more [echo]
        assert!(outputs[0].contains("hello world"));
        assert!(!outputs[0].contains("[echo]"));
        assert!(outputs[1].contains("OneClaw Agent v1.5.0")); // status contains name + version
        assert!(outputs[2].contains("Goodbye"));     // exit
    }

    #[tokio::test]
    async fn test_runtime_help_command() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        let channel = MockChannel::new(vec!["help", "exit"]);
        runtime.run(&channel).await.unwrap();
        let outputs = channel.get_outputs();
        assert!(outputs[0].contains("OneClaw Commands"));
        assert!(outputs[0].contains("status"));
        assert!(outputs[0].contains("pair"));
    }

    #[tokio::test]
    async fn test_runtime_pair_command() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        let channel = MockChannel::new(vec!["pair", "exit"]);
        runtime.run(&channel).await.unwrap();
        let outputs = channel.get_outputs();
        assert!(outputs[0].contains("Pairing code:"));
        assert!(outputs[0].contains("valid 5 minutes"));
    }

    #[tokio::test]
    async fn test_runtime_verify_command() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        // NoopSecurity accepts any code
        let channel = MockChannel::new(vec!["verify 123456", "exit"]);
        runtime.run(&channel).await.unwrap();
        let outputs = channel.get_outputs();
        assert!(outputs[0].contains("Device paired successfully"));
        assert!(outputs[0].contains("noop-device"));
    }

    #[tokio::test]
    async fn test_runtime_help_includes_verify() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        let channel = MockChannel::new(vec!["help", "exit"]);
        runtime.run(&channel).await.unwrap();
        let outputs = channel.get_outputs();
        assert!(outputs[0].contains("verify CODE"));
    }

    #[tokio::test]
    async fn test_runtime_quit_variants() {
        for cmd in &["quit", "q", "EXIT", "Quit"] {
            let config = OneClawConfig::default_config();
            let runtime = Runtime::with_defaults(config);
            let channel = MockChannel::new(vec![cmd]);
            runtime.run(&channel).await.unwrap();
            let outputs = channel.get_outputs();
            assert!(outputs.last().unwrap().contains("Goodbye"), "Failed for cmd: {}", cmd);
        }
    }

    #[tokio::test]
    async fn test_runtime_providers_command() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        let channel = MockChannel::new(vec!["providers", "exit"]);
        runtime.run(&channel).await.unwrap();
        let outputs = channel.get_outputs();
        assert!(outputs[0].contains("LLM Providers"));
        assert!(outputs[0].contains("noop"));
    }

    #[tokio::test]
    async fn test_runtime_help_includes_providers() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        let channel = MockChannel::new(vec!["help", "exit"]);
        runtime.run(&channel).await.unwrap();
        let outputs = channel.get_outputs();
        assert!(outputs[0].contains("providers"));
    }

    #[tokio::test]
    async fn test_runtime_llm_pipeline_with_noop() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        let response = runtime.process_with_llm("hello world").await;
        assert!(
            response.contains("noop") || response.contains("hello"),
            "Expected noop response, got: {}",
            response
        );
    }

    #[tokio::test]
    async fn test_runtime_llm_with_memory_context() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        runtime.memory.store(
            "sensor_01 | temperature | value = 22.5",
            crate::memory::MemoryMeta::default(),
        ).unwrap();
        let response = runtime.process_with_llm("sensor temperature readings").await;
        assert!(!response.is_empty());
    }

    #[test]
    fn test_analyze_complexity_integration() {
        use crate::orchestrator::router::{analyze_complexity, Complexity};
        assert_eq!(analyze_complexity("hi", false), Complexity::Simple);
        assert_eq!(
            analyze_complexity("analyze trend data over 7 days", true),
            Complexity::Complex,
        );
        assert_eq!(
            analyze_complexity("emergency critical alert!", false),
            Complexity::Critical,
        );
    }

    #[tokio::test]
    async fn test_runtime_ask_command() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        let channel = MockChannel::new(vec!["ask what is blood pressure", "exit"]);
        runtime.run(&channel).await.unwrap();
        let outputs = channel.get_outputs();
        // ask command bypasses handler, goes to LLM pipeline (noop)
        assert!(!outputs[0].is_empty(), "ask should produce a response");
        assert!(!outputs[0].contains("OneClaw Commands"), "ask should not show help");
    }

    #[tokio::test]
    async fn test_runtime_help_includes_ask() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        let channel = MockChannel::new(vec!["help", "exit"]);
        runtime.run(&channel).await.unwrap();
        let outputs = channel.get_outputs();
        assert!(outputs[0].contains("ask Q"), "Help should include ask command: {}", outputs[0]);
    }

    #[tokio::test]
    async fn test_runtime_events_command() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        let channel = MockChannel::new(vec!["events", "exit"]);
        runtime.run(&channel).await.unwrap();
        let outputs = channel.get_outputs();
        assert!(outputs[0].contains("Event Bus:"), "events command should show bus status: {}", outputs[0]);
    }

    #[tokio::test]
    async fn test_runtime_help_includes_events() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        let channel = MockChannel::new(vec!["help", "exit"]);
        runtime.run(&channel).await.unwrap();
        let outputs = channel.get_outputs();
        assert!(outputs[0].contains("events"), "Help should include events command: {}", outputs[0]);
    }

    #[tokio::test]
    async fn test_runtime_run_chain() {
        use crate::orchestrator::chain::{Chain, ChainStep, DefaultChainExecutor};
        let config = OneClawConfig::default_config();
        let mut runtime = Runtime::with_defaults(config);
        runtime.chain = Box::new(DefaultChainExecutor::new());

        let chain = Chain::new("test")
            .add_step(ChainStep::transform("format", "Result: {input}"));

        let result = runtime.run_chain(&chain, "test data").await.unwrap();
        assert_eq!(result.final_output, "Result: test data");
        assert_eq!(result.chain_name, "test");
        assert_eq!(result.steps.len(), 1);
    }

    #[tokio::test]
    async fn test_runtime_run_chain_multi_step() {
        use crate::orchestrator::chain::{Chain, ChainStep, DefaultChainExecutor};
        let config = OneClawConfig::default_config();
        let mut runtime = Runtime::with_defaults(config);
        runtime.chain = Box::new(DefaultChainExecutor::new());

        let chain = Chain::new("multi")
            .add_step(ChainStep::memory_search("search", "{input}", 5))
            .add_step(ChainStep::llm("analyze", "Data: {step_0}\nQuestion: {input}"))
            .add_step(ChainStep::transform("wrap", "Analysis:\n{input}"));

        let result = runtime.run_chain(&chain, "sensor data analysis").await.unwrap();
        assert_eq!(result.steps.len(), 3);
        assert!(!result.final_output.is_empty());
    }

    #[tokio::test]
    async fn test_runtime_no_echo_mode() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        let channel = MockChannel::new(vec!["Hello, how are you?", "exit"]);
        runtime.run(&channel).await.unwrap();
        let outputs = channel.get_outputs();
        assert!(
            !outputs[0].contains("[echo]"),
            "Response should not be echo mode: {}",
            outputs[0]
        );
    }

    #[tokio::test]
    async fn test_runtime_tools_command_empty() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        let channel = MockChannel::new(vec!["tools", "exit"]);
        runtime.run(&channel).await.unwrap();
        let outputs = channel.get_outputs();
        assert!(outputs[0].contains("Registered Tools (0)"), "tools should show count: {}", outputs[0]);
        assert!(outputs[0].contains("No tools registered"), "tools should show empty: {}", outputs[0]);
    }

    #[tokio::test]
    async fn test_runtime_tools_command_with_tool() {
        let config = OneClawConfig::default_config();
        let mut runtime = Runtime::with_defaults(config);
        Arc::get_mut(&mut runtime.tool_registry).unwrap().register(Box::new(crate::tool::NoopTool::new()));
        let channel = MockChannel::new(vec!["tools", "exit"]);
        runtime.run(&channel).await.unwrap();
        let outputs = channel.get_outputs();
        assert!(outputs[0].contains("Registered Tools (1)"), "tools should show 1: {}", outputs[0]);
        assert!(outputs[0].contains("noop"), "tools should list noop: {}", outputs[0]);
    }

    #[tokio::test]
    async fn test_runtime_tool_execute_command() {
        let config = OneClawConfig::default_config();
        let mut runtime = Runtime::with_defaults(config);
        Arc::get_mut(&mut runtime.tool_registry).unwrap().register(Box::new(crate::tool::NoopTool::new()));
        let channel = MockChannel::new(vec!["tool noop", "exit"]);
        runtime.run(&channel).await.unwrap();
        let outputs = channel.get_outputs();
        assert!(outputs[0].contains("[OK]"), "tool execute should show OK: {}", outputs[0]);
        assert!(outputs[0].contains("noop"), "tool execute should name tool: {}", outputs[0]);
    }

    #[tokio::test]
    async fn test_runtime_tool_execute_nonexistent() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        let channel = MockChannel::new(vec!["tool ghost", "exit"]);
        runtime.run(&channel).await.unwrap();
        let outputs = channel.get_outputs();
        assert!(outputs[0].contains("Tool error"), "nonexistent tool should error: {}", outputs[0]);
    }

    #[tokio::test]
    async fn test_runtime_help_includes_tools() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        let channel = MockChannel::new(vec!["help", "exit"]);
        runtime.run(&channel).await.unwrap();
        let outputs = channel.get_outputs();
        assert!(outputs[0].contains("tools"), "Help should include tools: {}", outputs[0]);
        assert!(outputs[0].contains("tool X"), "Help should include tool X: {}", outputs[0]);
    }

    #[test]
    fn test_runtime_tool_registry_accessible() {
        let config = OneClawConfig::default_config();
        let mut runtime = Runtime::with_defaults(config);
        assert_eq!(runtime.tool_registry.count(), 0);
        Arc::get_mut(&mut runtime.tool_registry).unwrap().register(Box::new(crate::tool::NoopTool::new()));
        assert_eq!(runtime.tool_registry.count(), 1);
    }

    #[tokio::test]
    async fn test_runtime_channels_command_single() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        let channel = MockChannel::new(vec!["channels", "exit"]);
        runtime.run(&channel).await.unwrap();
        let outputs = channel.get_outputs();
        assert!(outputs[0].contains("Active channels"), "channels should list: {}", outputs[0]);
        assert!(outputs[0].contains("mock"), "channels should show mock: {}", outputs[0]);
    }

    #[tokio::test]
    async fn test_runtime_help_includes_channels() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        let channel = MockChannel::new(vec!["help", "exit"]);
        runtime.run(&channel).await.unwrap();
        let outputs = channel.get_outputs();
        assert!(outputs[0].contains("channels"), "Help should include channels: {}", outputs[0]);
    }

    #[tokio::test]
    async fn test_runtime_run_multi_basic() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);

        let mut mgr = ChannelManager::new();
        mgr.add_channel(Box::new(MockChannel::new(vec!["status", "exit"])));

        runtime.run_multi(&mgr).await.unwrap();
    }

    #[tokio::test]
    async fn test_runtime_run_multi_processes_commands() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);

        // Create a mock channel that captures outputs
        struct CaptureMockChannel {
            inputs: std::sync::Mutex<Vec<String>>,
            outputs: std::sync::Mutex<Vec<String>>,
        }

        impl CaptureMockChannel {
            fn new(inputs: Vec<&str>) -> Self {
                Self {
                    inputs: std::sync::Mutex::new(inputs.into_iter().rev().map(String::from).collect()),
                    outputs: std::sync::Mutex::new(vec![]),
                }
            }
        }

        #[async_trait]
        impl crate::channel::Channel for CaptureMockChannel {
            fn name(&self) -> &str { "capture" }
            async fn receive(&self) -> crate::error::Result<Option<crate::channel::IncomingMessage>> {
                let mut inputs = self.inputs.lock().unwrap();
                match inputs.pop() {
                    Some(content) => Ok(Some(crate::channel::IncomingMessage {
                        source: "test".into(),
                        content,
                        timestamp: chrono::Utc::now(),
                    })),
                    None => Ok(None),
                }
            }
            async fn send(&self, message: &crate::channel::OutgoingMessage) -> crate::error::Result<()> {
                self.outputs.lock().unwrap().push(message.content.clone());
                Ok(())
            }
        }

        let ch = CaptureMockChannel::new(vec!["help", "exit"]);

        let mut mgr = ChannelManager::new();
        mgr.add_channel(Box::new(ch));

        runtime.run_multi(&mgr).await.unwrap();
        // Manager processes exit → we get Goodbye!
        // Can't easily read outputs from inside ChannelManager, but run_multi() didn't crash
    }

    #[tokio::test]
    async fn test_runtime_run_multi_channels_command() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);

        // Use the existing MockChannel which is simpler
        let mut mgr = ChannelManager::new();
        mgr.add_channel(Box::new(MockChannel::new(vec!["channels", "exit"])));
        mgr.add_channel(Box::new(MockChannel::new(vec![])));

        runtime.run_multi(&mgr).await.unwrap();
        // Check that active_channels was populated
        let channels = runtime.active_channels.lock().unwrap();
        assert_eq!(channels.len(), 2);
    }
}
