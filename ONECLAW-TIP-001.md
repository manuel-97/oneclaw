# Vibecode Kit v5.0 — Task Instruction Pack

## VAI TRÒ
Bạn là THỢ THI CÔNG trong hệ thống Vibecode Kit v5.0.
Chủ thầu (Claude Chat) và Chủ nhà đã THỐNG NHẤT bản thiết kế.

## QUY TẮC TUYỆT ĐỐI
1. IMPLEMENT ĐÚNG TIP specification bên dưới
2. KHÔNG thay đổi kiến trúc / structure
3. KHÔNG thêm features ngoài TIP
4. KHÔNG đổi tech stack / dependencies (trừ khi TIP yêu cầu)
5. SELF-TEST theo acceptance criteria
6. BÁO CÁO theo Completion Report format
7. Gặp conflict → BÁO CÁO chi tiết, KHÔNG tự quyết định

## PROJECT CONTEXT
- **Project:** OneClaw — Rust AI Agent Kernel cho Edge/IoT
- **Codebase:** MỚI HOÀN TOÀN — chưa có file nào
- **Rust Edition:** 2024
- **License:** Dual MIT/Apache 2.0
- **Architecture:** 5-layer trait-driven (Security → LLM Orchestrator → Memory → Event Bus → Tool/Channel)
- **Target:** <5MB RAM, <10ms boot, ARM64 + x86_64

---

# TIP-001: Cargo Workspace Scaffold

## HEADER
- **TIP-ID:** TIP-001
- **Project:** OneClaw
- **Module:** Foundation
- **Depends on:** None (first task)
- **Priority:** P0
- **Estimated effort:** 30-45 phút

## CONTEXT
- Working directory: Tạo mới tại `~/oneclaw/` (hoặc path Chủ nhà chỉ định)
- Key files to reference: Blueprint structure bên dưới
- Patterns to follow: Standard Rust workspace conventions

## TASK
Tạo Cargo workspace scaffold cho OneClaw với đầy đủ crate structure, dependencies tối thiểu, và đảm bảo `cargo build` + `cargo test` pass ngay từ đầu. Mỗi crate có stub code (compiles nhưng chưa có logic thật).

## SPECIFICATIONS

### 1. Workspace Root — `Cargo.toml`
```toml
[workspace]
resolver = "2"
members = [
    "crates/oneclaw-core",
    "crates/oneclaw-providers",
    "crates/oneclaw-channels",
    "crates/oneclaw-tools",
    "crates/oneclaw-elderly",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT OR Apache-2.0"
rust-version = "1.85"
repository = "https://github.com/nicekid1/oneclaw"
description = "AI Agent Kernel for Edge/IoT — LLM Orchestration done right"

[workspace.dependencies]
# Async
tokio = { version = "1", features = ["full"] }
# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
# Error handling
thiserror = "2"
anyhow = "1"
# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
# Crypto
ring = "0.17"
# Database (chưa dùng Sprint 1, khai báo sẵn)
rusqlite = { version = "0.32", features = ["bundled", "fts5"] }
# HTTP (chưa dùng Sprint 1, khai báo sẵn)
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
# Time
chrono = { version = "0.4", features = ["serde"] }
# UUID
uuid = { version = "1", features = ["v4", "serde"] }
# Bytes
bytes = "1"
```

### 2. Rust Toolchain — `rust-toolchain.toml`
```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
targets = ["aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"]
```

### 3. Cargo Config — `.cargo/config.toml`
```toml
[build]
# Optimize for size in release
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
strip = true
panic = "abort"
```
Lưu ý: `[profile.release]` thực tế phải nằm trong workspace root `Cargo.toml`, không phải `.cargo/config.toml`. Đặt đúng vị trí.

### 4. Crate: `oneclaw-core`
```toml
# crates/oneclaw-core/Cargo.toml
[package]
name = "oneclaw-core"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "OneClaw Core — 5-layer trait-driven AI Agent Kernel"

[dependencies]
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
toml = { workspace = true }
thiserror = { workspace = true }
anyhow = { workspace = true }
tracing = { workspace = true }
chrono = { workspace = true }
uuid = { workspace = true }
bytes = { workspace = true }
ring = { workspace = true }
```

**Source files — tạo đúng theo structure này:**

**`src/lib.rs`** — Re-export tất cả modules:
```rust
//! OneClaw Core — 5-layer trait-driven AI Agent Kernel
//!
//! Architecture:
//! - Layer 0: Security Core (Immune System)
//! - Layer 1: LLM Orchestrator (Heart) ★ MOAT
//! - Layer 2: Memory (Brain)
//! - Layer 3: Event Bus (Nervous System)
//! - Layer 4: Tool (Hands)
//! - Layer 5: Channel (Interface)

pub mod error;
pub mod config;
pub mod security;
pub mod orchestrator;
pub mod memory;
pub mod event_bus;
pub mod tool;
pub mod channel;
pub mod runtime;
```

**`src/error.rs`** — Unified error type:
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum OneClawError {
    #[error("Security: {0}")]
    Security(String),

    #[error("Orchestrator: {0}")]
    Orchestrator(String),

    #[error("Memory: {0}")]
    Memory(String),

    #[error("EventBus: {0}")]
    EventBus(String),

    #[error("Tool: {0}")]
    Tool(String),

    #[error("Channel: {0}")]
    Channel(String),

    #[error("Config: {0}")]
    Config(String),

    #[error("IO: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization: {0}")]
    Serde(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, OneClawError>;
```

**`src/config.rs`** — Config loader stub:
```rust
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Default)]
pub struct OneClawConfig {
    pub security: SecurityConfig,
    pub runtime: RuntimeConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct SecurityConfig {
    #[serde(default = "default_true")]
    pub deny_by_default: bool,
    #[serde(default = "default_true")]
    pub pairing_required: bool,
    #[serde(default = "default_true")]
    pub workspace_only: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct RuntimeConfig {
    #[serde(default = "default_name")]
    pub name: String,
    #[serde(default)]
    pub log_level: String,
}

fn default_true() -> bool { true }
fn default_name() -> String { "oneclaw".to_string() }

impl OneClawConfig {
    pub fn load(path: impl AsRef<Path>) -> crate::error::Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| crate::error::OneClawError::Config(
                format!("Failed to read config: {}", e)
            ))?;
        let config: Self = toml::from_str(&content)
            .map_err(|e| crate::error::OneClawError::Config(
                format!("Failed to parse config: {}", e)
            ))?;
        Ok(config)
    }

    /// Load with defaults (no file needed)
    pub fn default_config() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = OneClawConfig::default_config();
        assert!(config.security.deny_by_default);
        assert!(config.security.pairing_required);
        assert!(config.security.workspace_only);
    }

    #[test]
    fn test_load_from_toml_string() {
        let toml_str = r#"
[security]
deny_by_default = true
pairing_required = false

[runtime]
name = "test-agent"
log_level = "debug"
"#;
        let config: OneClawConfig = toml::from_str(toml_str).unwrap();
        assert!(config.security.deny_by_default);
        assert!(!config.security.pairing_required);
        assert_eq!(config.runtime.name, "test-agent");
    }
}
```

**`src/security.rs`** — Layer 0 trait + Noop:
```rust
//! Layer 0: Security Core — Immune System
//! Deny-by-default. Every action must be authorized.

use crate::error::Result;

/// Action that requires authorization
#[derive(Debug, Clone)]
pub struct Action {
    pub kind: ActionKind,
    pub resource: String,
    pub actor: String,
}

#[derive(Debug, Clone)]
pub enum ActionKind {
    Read,
    Write,
    Execute,
    Network,
    PairDevice,
}

/// Authorization permit
#[derive(Debug, Clone)]
pub struct Permit {
    pub granted: bool,
    pub reason: String,
}

/// Device identity after pairing
#[derive(Debug, Clone)]
pub struct Identity {
    pub device_id: String,
    pub paired_at: chrono::DateTime<chrono::Utc>,
}

/// Layer 0 Trait: Security Core
pub trait SecurityCore: Send + Sync {
    /// Authorize an action. Deny-by-default.
    fn authorize(&self, action: &Action) -> Result<Permit>;

    /// Check if a filesystem path is allowed
    fn check_path(&self, path: &std::path::Path) -> Result<()>;

    /// Generate a one-time pairing code
    fn generate_pairing_code(&self) -> Result<String>;

    /// Verify a pairing code and return device identity
    fn verify_pairing_code(&self, code: &str) -> Result<Identity>;
}

/// NoopSecurity: Allows everything. FOR TESTING ONLY.
pub struct NoopSecurity;

impl SecurityCore for NoopSecurity {
    fn authorize(&self, _action: &Action) -> Result<Permit> {
        Ok(Permit { granted: true, reason: "noop: all allowed".into() })
    }

    fn check_path(&self, _path: &std::path::Path) -> Result<()> {
        Ok(())
    }

    fn generate_pairing_code(&self) -> Result<String> {
        Ok("000000".to_string())
    }

    fn verify_pairing_code(&self, _code: &str) -> Result<Identity> {
        Ok(Identity {
            device_id: "noop-device".into(),
            paired_at: chrono::Utc::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_security_allows_all() {
        let sec = NoopSecurity;
        let action = Action {
            kind: ActionKind::Read,
            resource: "/some/path".into(),
            actor: "test".into(),
        };
        let permit = sec.authorize(&action).unwrap();
        assert!(permit.granted);
    }

    #[test]
    fn test_noop_pairing() {
        let sec = NoopSecurity;
        let code = sec.generate_pairing_code().unwrap();
        let identity = sec.verify_pairing_code(&code).unwrap();
        assert_eq!(identity.device_id, "noop-device");
    }
}
```

**`src/orchestrator/mod.rs`** — Layer 1 trait stubs:
```rust
//! Layer 1: LLM Orchestrator — Heart ★ MOAT
//! Smart routing, chain execution, context management, graceful degradation.

pub mod router;
pub mod context;
pub mod chain;
pub mod fallback;

// Re-exports
pub use router::ModelRouter;
pub use context::ContextManager;
pub use chain::ChainExecutor;
pub use fallback::DegradationMode;
```

Tạo 4 file con trong `src/orchestrator/`:

**`router.rs`**:
```rust
use crate::error::Result;

/// How complex is this task?
#[derive(Debug, Clone, Copy)]
pub enum Complexity {
    Simple,    // Local tiny model
    Medium,    // Local large or cloud cheap
    Complex,   // Cloud smart (Sonnet)
    Critical,  // Cloud best (Opus) + verify
}

/// Which model to use
#[derive(Debug, Clone)]
pub struct ModelChoice {
    pub provider: String,
    pub model: String,
    pub reason: String,
}

pub trait ModelRouter: Send + Sync {
    fn route(&self, complexity: Complexity) -> Result<ModelChoice>;
}

/// Noop: always returns a placeholder
pub struct NoopRouter;
impl ModelRouter for NoopRouter {
    fn route(&self, _complexity: Complexity) -> Result<ModelChoice> {
        Ok(ModelChoice {
            provider: "noop".into(),
            model: "noop".into(),
            reason: "noop router".into(),
        })
    }
}
```

**`context.rs`**:
```rust
use crate::error::Result;

pub trait ContextManager: Send + Sync {
    fn assemble(&self, task: &str, budget_tokens: usize) -> Result<String>;
    fn compress(&self, context: &str, target_tokens: usize) -> Result<String>;
}

pub struct NoopContextManager;
impl ContextManager for NoopContextManager {
    fn assemble(&self, task: &str, _budget: usize) -> Result<String> {
        Ok(task.to_string())
    }
    fn compress(&self, context: &str, _target: usize) -> Result<String> {
        Ok(context.to_string())
    }
}
```

**`chain.rs`**:
```rust
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct Step {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct StepResult {
    pub step_id: String,
    pub output: String,
    pub confidence: f32,
}

pub trait ChainExecutor: Send + Sync {
    fn plan(&self, goal: &str) -> Result<Vec<Step>>;
    fn execute_step(&self, step: &Step) -> Result<StepResult>;
}

pub struct NoopChainExecutor;
impl ChainExecutor for NoopChainExecutor {
    fn plan(&self, goal: &str) -> Result<Vec<Step>> {
        Ok(vec![Step { id: "1".into(), description: goal.into() }])
    }
    fn execute_step(&self, step: &Step) -> Result<StepResult> {
        Ok(StepResult {
            step_id: step.id.clone(),
            output: format!("noop: {}", step.description),
            confidence: 1.0,
        })
    }
}
```

**`fallback.rs`**:
```rust
/// Degradation modes when connectivity drops
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DegradationMode {
    /// Full connectivity: use cloud for complex tasks
    FullOnline,
    /// Limited: only cloud for Critical tasks
    Metered,
    /// No internet: 100% local models
    Offline,
    /// No models at all: rule-based fallback
    Emergency,
}

impl Default for DegradationMode {
    fn default() -> Self { Self::FullOnline }
}
```

**`src/memory/mod.rs`** — Layer 2 stubs:
```rust
//! Layer 2: Memory — Brain
//! Tri-Search: Vector + FTS5 + Temporal

pub mod traits;

pub use traits::{Memory, NoopMemory};
```

**`src/memory/traits.rs`**:
```rust
use crate::error::Result;

pub trait Memory: Send + Sync {
    fn store(&self, content: &str, tags: &[&str]) -> Result<String>; // returns ID
    fn search(&self, query: &str, limit: usize) -> Result<Vec<String>>;
}

pub struct NoopMemory;
impl Memory for NoopMemory {
    fn store(&self, _content: &str, _tags: &[&str]) -> Result<String> {
        Ok(uuid::Uuid::new_v4().to_string())
    }
    fn search(&self, _query: &str, _limit: usize) -> Result<Vec<String>> {
        Ok(vec![])
    }
}
```

**`src/event_bus/mod.rs`** — Layer 3 stubs:
```rust
//! Layer 3: Event Bus — Nervous System

pub mod traits;

pub use traits::{EventBus, NoopEventBus};
```

**`src/event_bus/traits.rs`**:
```rust
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct Event {
    pub topic: String,
    pub payload: Vec<u8>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

pub trait EventBus: Send + Sync {
    fn publish(&self, event: Event) -> Result<()>;
    fn subscribe(&self, topic: &str) -> Result<()>;
}

pub struct NoopEventBus;
impl EventBus for NoopEventBus {
    fn publish(&self, _event: Event) -> Result<()> { Ok(()) }
    fn subscribe(&self, _topic: &str) -> Result<()> { Ok(()) }
}
```

**`src/tool/mod.rs`** — Layer 4 stubs:
```rust
//! Layer 4: Tool — Hands

pub mod traits;

pub use traits::{Tool, NoopTool};
```

**`src/tool/traits.rs`**:
```rust
use crate::error::Result;

pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn execute(&self, action: &str, params: &[u8]) -> Result<Vec<u8>>;
}

pub struct NoopTool;
impl Tool for NoopTool {
    fn name(&self) -> &str { "noop" }
    fn description(&self) -> &str { "No-op tool for testing" }
    fn execute(&self, _action: &str, _params: &[u8]) -> Result<Vec<u8>> {
        Ok(vec![])
    }
}
```

**`src/channel/mod.rs`** — Layer 5 stubs:
```rust
//! Layer 5: Channel — Interface

pub mod traits;

pub use traits::{Channel, NoopChannel};
```

**`src/channel/traits.rs`**:
```rust
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub source: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct OutgoingMessage {
    pub destination: String,
    pub content: String,
}

pub trait Channel: Send + Sync {
    fn name(&self) -> &str;
    fn receive(&self) -> Result<Option<IncomingMessage>>;
    fn send(&self, message: &OutgoingMessage) -> Result<()>;
}

pub struct NoopChannel;
impl Channel for NoopChannel {
    fn name(&self) -> &str { "noop" }
    fn receive(&self) -> Result<Option<IncomingMessage>> { Ok(None) }
    fn send(&self, _message: &OutgoingMessage) -> Result<()> { Ok(()) }
}
```

**`src/runtime.rs`** — Bootstrap:
```rust
//! OneClaw Runtime — Main event loop

use crate::config::OneClawConfig;
use crate::security::{SecurityCore, NoopSecurity};
use crate::orchestrator::router::{ModelRouter, NoopRouter};
use crate::orchestrator::context::{ContextManager, NoopContextManager};
use crate::orchestrator::chain::{ChainExecutor, NoopChainExecutor};
use crate::memory::{Memory, NoopMemory};
use crate::event_bus::{EventBus, NoopEventBus};
use crate::error::Result;
use tracing::info;

pub struct Runtime {
    pub config: OneClawConfig,
    pub security: Box<dyn SecurityCore>,
    pub router: Box<dyn ModelRouter>,
    pub context_mgr: Box<dyn ContextManager>,
    pub chain: Box<dyn ChainExecutor>,
    pub memory: Box<dyn Memory>,
    pub event_bus: Box<dyn EventBus>,
}

impl Runtime {
    /// Create runtime with all Noop implementations (for testing / bare boot)
    pub fn with_defaults(config: OneClawConfig) -> Self {
        Self {
            config,
            security: Box::new(NoopSecurity),
            router: Box::new(NoopRouter),
            context_mgr: Box::new(NoopContextManager),
            chain: Box::new(NoopChainExecutor),
            memory: Box::new(NoopMemory),
            event_bus: Box::new(NoopEventBus),
        }
    }

    /// Boot the runtime
    pub fn boot(&self) -> Result<()> {
        info!(
            name = %self.config.runtime.name,
            deny_by_default = %self.config.security.deny_by_default,
            "OneClaw runtime booting"
        );
        info!("Layer 0: Security Core ✓");
        info!("Layer 1: LLM Orchestrator ✓");
        info!("Layer 2: Memory ✓");
        info!("Layer 3: Event Bus ✓");
        info!("Runtime ready. All 5 layers initialized.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_boots_with_defaults() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        // Just ensure boot doesn't panic
        // (tracing not initialized in test, so info! is silent — that's fine)
        assert!(runtime.boot().is_ok());
    }

    #[test]
    fn test_runtime_config_accessible() {
        let config = OneClawConfig::default_config();
        let runtime = Runtime::with_defaults(config);
        assert!(runtime.config.security.deny_by_default);
    }
}
```

### 5. Crate: `oneclaw-providers` (stub)
```toml
[package]
name = "oneclaw-providers"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "LLM Provider implementations for OneClaw"

[dependencies]
oneclaw-core = { path = "../oneclaw-core" }
tokio = { workspace = true }
reqwest = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
```

**`src/lib.rs`**:
```rust
//! OneClaw Providers — LLM provider implementations
//!
//! Sprint 1: Stub only. Implementations in Sprint 5-6.

pub mod ollama;
pub mod openai_compat;
```

**`src/ollama.rs`** và **`src/openai_compat.rs`**: File rỗng với comment:
```rust
//! Ollama provider — Sprint 5-6 implementation
//! TODO: Implement ModelRouter trait for Ollama
```

### 6. Crate: `oneclaw-channels` (stub)
Tương tự pattern trên. `Cargo.toml` depend `oneclaw-core`. `src/lib.rs` declare modules `cli`, `mqtt`, `telegram`. Mỗi module là file rỗng với TODO comment.

### 7. Crate: `oneclaw-tools` (stub)
Tương tự. Modules: `file_ops`, `shell`, `http_client`, `notify`.

### 8. Crate: `oneclaw-elderly` (stub)
```toml
[package]
name = "oneclaw-elderly"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "OneClaw Elderly Care Agent — Reference Vertical"

[[bin]]
name = "oneclaw-elderly"
path = "src/main.rs"

[dependencies]
oneclaw-core = { path = "../oneclaw-core" }
tokio = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
```

**`src/main.rs`**:
```rust
use oneclaw_core::config::OneClawConfig;
use oneclaw_core::runtime::Runtime;
use tracing::info;

fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    info!("OneClaw Elderly Care Agent starting...");

    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    match runtime.boot() {
        Ok(()) => info!("Elderly Care Agent ready."),
        Err(e) => eprintln!("Boot failed: {}", e),
    }
}
```

**`src/lib.rs`**: Module declarations with TODO:
```rust
//! OneClaw Elderly Care — Reference Vertical
//! Modules to be implemented in subsequent sprints.
```

### 9. Root files

**`LICENSE-MIT`**: Standard MIT license text, copyright "2026 OneClaw Contributors"

**`LICENSE-APACHE`**: Standard Apache 2.0 text

**`README.md`**:
```markdown
# OneClaw 🦀

**AI Agent Kernel for Edge/IoT — LLM Orchestration done right.**

OneClaw is a 5-layer trait-driven AI agent kernel written in Rust. It's designed
to run on edge hardware (<5MB RAM, <10ms boot) while orchestrating LLMs
intelligently — not just calling APIs.

## Architecture

| Layer | Name | Purpose |
|-------|------|---------|
| 0 | Security Core | Deny-by-default, encryption, pairing |
| 1 | LLM Orchestrator ★ | Smart routing, chain execution, fallback |
| 2 | Memory | Tri-Search (vector + keyword + temporal) |
| 3 | Event Bus | Pub/sub, sensor streams, alert pipelines |
| 4 | Tool | Sandboxed actions (shell, GPIO, HTTP) |
| 5 | Channel | I/O interface (CLI, MQTT, Telegram) |

## Quick Start

```bash
cargo build --release
./target/release/oneclaw-elderly
```

## License

Dual-licensed under MIT and Apache 2.0.
```

**`config/default.toml`** (tạo ở root, reference config):
```toml
# OneClaw Default Configuration

[security]
deny_by_default = true
pairing_required = true
workspace_only = true

[runtime]
name = "oneclaw"
log_level = "info"
```

### 10. `.gitignore`
```
/target
*.swp
*.swo
.env
.DS_Store
```

## ACCEPTANCE CRITERIA

```gherkin
Given: Workspace mới tạo xong
When: Chạy `cargo build --workspace`
Then: Build thành công, 0 errors

Given: Workspace đã build
When: Chạy `cargo test --workspace`
Then: Tất cả tests pass (ít nhất 6 tests từ core)

Given: Workspace đã build
When: Chạy `cargo clippy --workspace -- -D warnings`
Then: 0 warnings

Given: Workspace đã build
When: Chạy `cargo build --release -p oneclaw-elderly`
Then: Binary tạo ra < 2MB

Given: Binary đã build
When: Chạy `./target/release/oneclaw-elderly`
Then: Output log "OneClaw Elderly Care Agent starting..." và "Elderly Care Agent ready."

Given: Tất cả crate structure
When: Kiểm tra crates/oneclaw-core/src/
Then: Có đúng các file: lib.rs, error.rs, config.rs, security.rs, runtime.rs + 4 module dirs (orchestrator, memory, event_bus, tool, channel)

Given: Mọi trait trong core
When: Kiểm tra
Then: Mỗi trait có NoopImpl đi kèm
```

## CONSTRAINTS
- KHÔNG thêm dependency ngoài danh sách ở Specifications
- KHÔNG tạo file ngoài structure đã mô tả
- KHÔNG thêm logic business nào — chỉ trait definitions + noop impls
- Profile release PHẢI có `opt-level = "z"` + `lto = true` + `strip = true`
- Mỗi file .rs PHẢI có module-level doc comment (`//!`)

## REPORT FORMAT SAU KHI XONG

```markdown
### COMPLETION REPORT — TIP-001

**STATUS:** DONE / PARTIAL / BLOCKED

**FILES CHANGED:**
- Created: [list + purpose]

**TEST RESULTS:**
- Acceptance criteria tested: [X/Y passed]
- Details: [pass/fail cho từng criteria]

**BINARY SIZE:**
- `oneclaw-elderly` release: [X] KB

**ISSUES DISCOVERED:**
- [Issue]: [severity] — [description] — [suggestion]

**DEVIATIONS FROM SPEC:**
- [Deviation]: [what] — [why] — [impact]

**SUGGESTIONS FOR CHỦ THẦU:**
- [Suggestion]: [observation] — [recommendation]
```
