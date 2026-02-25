# oneclaw-core

Core runtime, traits, and registry for OneClaw AI Agent Kernel.

Contains all 5+1 layer trait definitions, default implementations, and the Runtime event loop.

## Layers

- `security/` — L0: Deny-by-default access control, device pairing, rate limiting, path guard
- `orchestrator/` — L1: Router (complexity analysis), Context Manager, Chain Executor, LLM Provider interface
- `memory/` — L2: Memory trait + SQLite/FTS5 backend + NoopMemory for testing
- `event_bus/` — L3: Pub/sub with topic patterns, declarative pipelines, ring-buffer history
- `tool/` — L4: Tool trait + ToolRegistry with param validation and event emission
- `channel/` — L5: Channel trait + ChannelManager for multi-source round-robin I/O

## Key Modules

- `runtime.rs` — Main event loop, command dispatch, LLM pipeline, metrics, graceful shutdown
- `config.rs` — TOML configuration with serde deserialization
- `metrics.rs` — 18 atomic counters for operational telemetry
- `registry.rs` — Trait resolver (config -> implementations)

## Usage

```rust
use oneclaw_core::config::OneClawConfig;
use oneclaw_core::runtime::Runtime;

let config = OneClawConfig::default_config();
let runtime = Runtime::with_defaults(config);
```
