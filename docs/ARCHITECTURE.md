# OneClaw Architecture

## Layer Interaction Flow

```
User/Sensor Input
       |
       v
  +-- Channel Layer --+
  |  CLI / TCP         |
  +--------+-----------+
           v
  +-- Security Gate ---+
  |  Rate Limit        |
  |  Device Pairing    |
  +--------+-----------+
           v
  +-- Runtime Router --+
  |  Command Dispatch  |
  |  or LLM Pipeline   |
  +--------+-----------+
           |
    +------+--------+----------------+
    v      v        v                v
 Memory   LLM    Chain           Event Bus
 Store    Call    Executor        Publish
 Search           (multi-step)   Pipeline
    |      |        |            Drain
    |      |        |                |
    +------+--------+                v
           |                  Alert Handler
           v                  -> Notify Tool
    Response via Channel
```

## 5+1 Layer Overview

### L0: Security Core (Immune System)

**Principle:** Deny-by-default. Everything is blocked unless explicitly permitted.

- **Device pairing:** Generate 6-digit code, verify within TTL, grant Identity
- **Rate limiting:** Token bucket per minute (default: 60 req/min)
- **Path guard:** Workspace-scoped file access, blocks system directories and dotfiles
- **API key masking:** Debug output never reveals full API keys
- **Always-open commands:** `help`, `pair`, `verify`, `exit` bypass security

### L1: LLM Orchestrator (Heart)

The intelligence layer routes, manages context, and executes multi-step reasoning.

- **Router:** Analyzes input complexity (Simple/Medium/Complex/Critical) using keyword + structural analysis. Vietnamese-aware with emergency detection.
- **Context Manager:** Builds LLM prompts with system prompt + memory context + conversation history. Token budget trimming.
- **Chain Executor:** Sequential multi-step chains with template variable substitution (`{input}`, `{step_N}`). Steps: LLM call, memory search, transform, emit event, tool call.
- **Provider Manager:** Pluggable LLM backends with fallback. Noop provider for offline mode.

### L2: Memory (Hippocampus)

Persistent, searchable storage with temporal awareness.

- **NoopMemory:** In-memory vector (testing/lightweight deployments)
- **SqliteMemory:** SQLite with FTS5 full-text search, `unicode61` tokenizer with diacritics removal for Vietnamese support
- **MemoryQuery:** Multi-dimensional search: text (FTS), tags (AND logic), time range, priority filter, limit
- **MemoryMeta:** Tags, priority (Low/Medium/High/Critical), source attribution

### L3: Event Bus (Nervous System)

Reactive pub/sub with declarative pipelines.

- **Synchronous drain model:** Events queue up, `drain()` dispatches to matching handlers. Handlers can generate response events (processed in next drain cycle).
- **Topic patterns:** Exact match, prefix wildcard (`vitals.*`), global wildcard (`*`)
- **Declarative pipelines:** FilterOp (HasField, FieldEquals, FieldGreaterThan, FieldLessThan) + PipelineAction (EmitEvent, SetField, Log)
- **Ring-buffer history:** Configurable max history (default: 100 events)

### L4: Tool (Hands)

Sandboxed external actions with parameter validation.

- **ToolRegistry:** Register/execute tools by name. Validates required parameters before execution.
- **Event emission:** Every tool execution publishes an event to `tool.<name>` topic with success/output metadata.
- **Built-in tools:** system_info (OS/memory), file_write (workspace-scoped), notify (caregiver alerts)

### L5: Channel (Ears/Mouth)

Multi-source I/O with round-robin polling.

- **CliChannel:** Interactive stdin/stdout with configurable prompt
- **TcpChannel:** Line-based TCP server for IoT sensors. One client at a time.
- **ChannelManager:** Multiplexes multiple channels with round-robin `receive_any()`

## Key Design Decisions

### Synchronous Architecture

Benchmarks prove 3.8M msg/sec synchronous throughput. Async (tokio) adds ~1MB binary size and complexity without measurable benefit for edge/IoT workloads where message rates are <100/sec. Decision: stay synchronous for the runtime event loop.

Note: tokio is still a workspace dependency (used by providers for HTTP calls) but the core event loop is synchronous.

### Trait-Driven Layering

Every layer is defined by a trait in `oneclaw-core`. Implementations are pluggable at construction time:

- `NoopMemory` for testing -> `SqliteMemory` for production
- `NoopProvider` for offline -> `OllamaProvider` for local LLM
- `CliChannel` for interactive -> `TcpChannel` for sensors
- `NoopSecurity` for tests -> `DefaultSecurity` for production

Three Runtime constructors:
- `with_defaults()` — All Noop (tests, minimal boot)
- `with_security()` — DefaultSecurity + Noop rest
- `from_config()` — Registry resolves all traits from TOML config

### Vertical Architecture

`oneclaw-core` defines generic agent infrastructure. Verticals (like `oneclaw-elderly`) wire domain-specific logic without modifying core. Core never imports from verticals.

Vertical customization points:
- `MessageHandler` — Domain-specific message processing (vitals parsing)
- `ChainHandler` — Domain-specific chain selection (health analysis)
- Event bus subscriptions — Domain-specific reactive pipelines (fever detection)
- Tool registration — Domain-specific tools

### Event-Driven Reactivity

Data flow: Vitals -> Events -> Pipelines -> Alerts -> Notifications

Declarative pipeline definitions (FilterOp + PipelineAction) enable domain experts to define monitoring rules without code changes. Example: "If temperature > 38.0, emit fever alert with Critical priority."

### Graceful Degradation

Every external dependency has a fallback:
- LLM provider offline -> Noop provider with `[Offline]` prefix
- Memory store fails -> Error message, session continues
- TCP port busy -> TCP channel disabled, CLI continues
- Config file missing -> Default configuration

### Metrics and Observability

18 atomic counters (zero-contention AtomicU64) track all operational aspects:
- Message flow: total, denied, rate-limited, secured
- LLM: calls, failures, latency, tokens
- Memory: stores, searches
- Tools: calls, failures
- Chains: executions, steps
- Events: processed
- Errors: total

Exposed via `status`, `health`, and `metrics` commands.
