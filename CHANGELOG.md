# Changelog

All notable changes to OneClaw are documented in this file.

## [1.6.0] — 2026-02-25 — SCALPEL

### Breaking Changes
- Removed legacy `LlmProvider` trait, `ProviderManager`, `LlmRequest`, `LlmResponse`
  types. Use `Provider` trait instead.
- Removed `oneclaw-providers` crate. All providers now in `oneclaw-core::provider`.
- Removed `DegradationMode` enum.
- `Runtime` no longer has `provider_mgr` field. Use `provider: Option<Box<dyn Provider>>`.
- `ChainContext` no longer has `provider_mgr` field. Use `provider: Option<&dyn Provider>`.

### Added
- **Vector Memory Search** — Semantic search using embeddings alongside FTS5 keyword search.
  - `Embedding` type with serialize/deserialize for SQLite BLOB storage.
  - `VectorMemory` trait extending `Memory` with `vector_search()` and `hybrid_search()`.
  - `cosine_similarity()` and `reciprocal_rank_fusion()` utility functions.
  - `SqliteMemory` implements `VectorMemory` with brute-force cosine similarity.
  - SQLite schema auto-migration adds embedding columns idempotently.
- **Embedding Providers** — Generate text embeddings for vector search.
  - `EmbeddingProvider` trait with `embed()` and `embed_batch()`.
  - `OllamaEmbedding` — local, offline-capable (nomic-embed-text 768d default).
  - `OpenAIEmbedding` — cloud-based (text-embedding-3-small 1536d default).
  - `build_embedding_provider()` factory from TOML config.
- **Auto-Embed Memory** — `remember` command auto-embeds with graceful fallback.
- **Semantic Recall** — `recall` command uses hybrid search (FTS + vector + RRF).
- **Semantic Context** — `process_with_llm()` uses hybrid search for memory context.
- **Async Event Bus** — `AsyncEventBus` using tokio broadcast channels.
  - Realtime event delivery (< 10ms latency, no drain() needed).
  - Multiple concurrent subscribers via `subscribe_channel()`.
  - Opt-in via `Runtime::with_async_event_bus()`. DefaultEventBus still default.
- **Per-Command Security** — All commands have individual authorization checks.
  Resources encoded in authorization (memory:write, memory:read, tool:{name}, etc.).
- `Memory::as_vector()` — upcast to `VectorMemory` if supported.
- `status` command shows embedding provider info and vector memory stats.

### Removed
- `oneclaw-providers` crate (superseded by `oneclaw-core::provider`).
- `LlmProvider`, `ProviderManager`, `LlmRequest`, `LlmResponse`, `DegradationMode`.
- All legacy async provider code paths.
- `llm_with_timeout()` helper.

### Fixed
- `remember` and `recall` commands no longer bypass security authorization.
- Generic auth check replaced with per-command authorization for all secured commands.

### Stats
- Workspace: 3 crates (was 4)
- Tests: 550+ (was 474)
- Clippy: 0 warnings
- Rustdoc: 0 warnings
- Binary: ~3.5MB

---

## [1.5.1] — 2026-02-25

### Pure Kernel — Domain-Agnostic

**TIP-SPLIT: Removed elderly care domain code from kernel**
- Kernel is now domain-agnostic — no hardcoded domain logic
- Elderly care code preserved in `oneclaw-care` repository
- Removed `oneclaw-elderly` crate (vitals parsing, care pipelines, health analysis chains)
- Removed `MessageHandler` and `ChainHandler` callback types from Runtime
- Removed `analyze` command (domain-specific chain execution)
- Removed `sensor_psk` from SecurityConfig
- Removed `DefaultContextManager::elderly_care()` factory
- Genericized system prompts, help text, and error messages
- Replaced domain-specific test data with generic sensor/device examples
- Updated `analyze_complexity()` to use domain-agnostic keywords

**Quality**
- All remaining tests pass
- 0 clippy warnings
- 0 rustdoc warnings
- Binary size: ~3.4MB

## [1.5.0] — 2026-02-22

### Smart Providers — 6 LLM Backends + FallbackChain

**Multi-Provider Foundation (TIP-031 to TIP-036)**
- 6 LLM providers: Anthropic Claude, OpenAI GPT, DeepSeek, Groq, Google Gemini, Ollama (local)
- `Provider` trait (sync): `chat`, `chat_with_history`, `is_available`, `id`, `display_name`
- `ReliableProvider`: automatic retry wrapper with configurable max retries
- `FallbackChain`: ordered failover across providers (primary → fallback[0] → ...)
- Per-provider API key config via `[provider.keys]` table
- `resolve_provider()`: routes config to concrete provider implementation
- `providers` command: list all configured providers and their status

**Per-Provider Configuration**
- `ProviderConfigToml`: primary, model, max_tokens, temperature, api_key, fallback chain
- API key resolution: config → ONECLAW_API_KEY → provider-specific env var
- Ollama endpoint/model override for local deployment
- Graceful degradation: no key → offline mode

**Cleanup (TIP-037)**
- 0 clippy warnings (fixed 10 in test code)
- 0 rustdoc warnings (fixed 8 bare URLs + bracket escapes)
- Updated CHANGELOG, README stats

**Quality**
- 532 tests, binary 3.4MB, 0 clippy warnings, 0 rustdoc warnings

---

## [1.2.0] — 2026-02-22

### Field Ready — Telegram, MQTT, ARM Deploy, Persistent Sessions

**Communication Channels (TIP-027, TIP-028)**
- Telegram bot channel: long-polling, chat ID whitelist, auto-split at 4000 chars
- `send_telegram_alert()` standalone function for one-shot alerts
- MQTT channel: rumqttc AsyncClient, configurable topics, QoS AtLeastOnce
- `clone_client()` for independent alert publishing
- Multi-channel alert dispatch: Notify + Telegram + MQTT (fire-and-forget)

**Edge Deployment (TIP-029)**
- ARM cross-compile: aarch64 + armv7 via `scripts/cross-build.sh`
- systemd service unit with 128M memory limit, unprivileged user
- Idempotent install/uninstall scripts
- `.cargo/config.toml` ARM linker hints

**Persistent Sessions (TIP-030)**
- `SqliteSecurityStore`: persist paired devices across reboots
- `devices` / `unpair` commands in Runtime
- Prefix match + ambiguity guard for device lookup

**Quality**
- 373 tests, binary 3.3MB, 0 clippy warnings

---

## [1.1.0] — 2026-02-22

### Async Runtime — Tokio Migration + Metrics + Health

**Tokio Async Migration (TIP-026)**
- Channel trait fully async (`async fn receive/send`)
- reqwest async HTTP client
- Tokio runtime for all I/O operations

**Observability**
- 18 AtomicU64 operational metrics (messages, LLM, memory, tools, events, chains, errors)
- `health` command: 5-layer system probe
- `metrics` command: full telemetry report
- Config reload with diff detection

**Quality**
- 344 tests, binary 3.04MB, 0 clippy warnings

---

## [1.0.0] — 2026-02-22

### Initial Release — AI Agent Kernel for Edge/IoT

**Architecture: 5+1 Layer Agent Runtime**
- L0 Security: Deny-by-default access control, device pairing, rate limiting (60/min), API key masking
- L1 Orchestrator: Smart Router (4-factor complexity), Context Manager, Chain Executor (multi-step LLM reasoning), LLM Provider abstraction
- L2 Memory: SQLite + FTS5 full-text search, Vietnamese-ready (unicode61 tokenizer), temporal queries, tag-based filtering
- L3 Event Bus: Synchronous pub/sub, topic pattern matching (wildcards), declarative pipelines (FilterOp -> PipelineAction), ring buffer history
- L4 Tool: Tool Registry with parameter validation, sandboxed execution, event emission. Built-in: system_info, file_write, notify
- L5 Channel: Multi-channel runtime (ChannelManager, round-robin polling). Built-in: CLI (interactive), TCP socket (IoT sensors)

**Vertical: Elderly Care Agent**
- Bilingual vital signs parser (Vietnamese + English): BP, temperature, heart rate, SpO2, weight, blood sugar
- Smart Recall with FTS5 search and temporal filtering
- Reactive health pipelines: fever detection (>38C), high BP (systolic >=140), low SpO2 (<95%)
- Health analysis chains: 4-step LLM reasoning (search -> analyze -> recommend -> notify)
- Sensor protocol: `SENSOR:type:patient:value` + JSON format via TCP
- Alert -> Notify tool auto-wiring

**LLM Integration**
- Ollama provider (local, offline-capable)
- OpenAI-compatible provider (OpenAI, Azure, Together, Groq, vLLM)
- Multi-provider fallback with graceful degradation
- NoopProvider for offline operation

**Observability**
- 18 AtomicU64 operational metrics (messages, LLM, memory, tools, events, chains, errors)
- `health` command: 5-layer system probe
- `metrics` command: full telemetry report
- `status` command: comprehensive overview
- Config reload diff detection

**Performance (Benchmarked)**
- Boot: 0.79us (target <10ms) — 12,600x margin
- Throughput: 3.8M msg/sec (target >1K) — 3,778x margin
- Events: 443K/sec (target >5K) — 88x margin
- Memory search: 11.9us (target <5ms) — 420x margin
- Binary: 3.08MB (target <5MB) — 1.66x margin

**Quality**
- 338 tests (unit + integration + benchmarks + adversarial)
- 63 edge case tests: Unicode, Vietnamese, CJK, RTL, null bytes, SQL injection, concurrent R/W — 0 bugs
- 220 public API doc comments
- Zero clippy warnings, zero dead code
- Graceful Mutex poison recovery
- LLM timeout monitoring
- Graceful shutdown (Arc<AtomicBool>)

### Sprint History

| Sprint | Focus | TIPs | Tests |
|--------|-------|------|-------|
| 1-2 | Foundation | TIP-001 to 005 | 38 |
| 3-4 | Memory | TIP-006 to 007 | 71 |
| 5-6 | Orchestrator | TIP-008 to 010 | 119 |
| 7-8 | Event Bus + Chain | TIP-011 to 013 | 168 |
| 9-10 | Tool + Channel | TIP-014 to 016 | 224 |
| 11-12 | Hardening | TIP-017 to 019 | 265 |
| 13-14 | Stress Test | TIP-020 to 022 | 338 |
| 15-16 | Polish + Release | TIP-023 to 025 | 338 |
