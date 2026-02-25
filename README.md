# OneClaw — Edge AI Agent Kernel

A lightweight, secure, trait-driven AI agent runtime built in Rust. Designed for resource-constrained edge devices: smart home hubs, industrial IoT gateways, agricultural sensor networks, and any domain needing AI + Edge + Realtime.

**Domain-agnostic** — use as foundation for any AI-powered IoT application.

## Key Numbers

| Metric | Target | Actual |
|--------|--------|--------|
| Boot time | <10ms | 0.79us |
| Binary size | <5MB | ~3.4MB |
| Message throughput | >1K/sec | 3.8M/sec |
| Event processing | >5K/sec | 443K/sec |
| Memory search | <5ms | 11.9us |
| Test coverage | — | 400+ tests |

## Architecture

```
+---------------------------------------------+
|              OneClaw Runtime                 |
+---------+----------+----------+-------------+
| Channel | Tool     | EventBus | Orchestrator|
| (Ears)  | (Hands)  | (Nerves) | (Brain)     |
+---------+----------+----------+-------------+
|              Memory (Hippocampus)            |
+---------------------------------------------+
|         Security Core (Immune System)        |
+---------------------------------------------+
```

### 6 Layers

| Layer | Role | Implementation |
|-------|------|---------------|
| L0 Security | Deny-by-default access control | Pairing, rate limiting, API key masking |
| L1 Orchestrator | LLM routing + multi-step reasoning | Router, Context Manager, Chain Executor |
| L2 Memory | Persistent storage + full-text search | SQLite + FTS5 |
| L3 Event Bus | Reactive pub/sub + pipelines | Synchronous drain, topic patterns, declarative pipelines |
| L4 Tool | Sandboxed external actions | Registry, param validation, system_info/file_write/notify |
| L5 Channel | Multi-source I/O | CLI, TCP, Telegram, MQTT — ChannelManager round-robin |

### 6 LLM Providers

Anthropic, OpenAI, DeepSeek, Groq, Gemini, Ollama — with FallbackChain auto-failover.

## Use Cases

- Smart Home automation
- Industrial IoT monitoring
- Agricultural sensor networks
- Healthcare devices
- Any domain needing AI + Edge + Realtime

## Quick Start

### Prerequisites

- Rust 1.85+ (edition 2024)
- No other dependencies required

### Build

```bash
cargo build --release
```

### Run

```bash
cargo run --release -p oneclaw-core
```

## Configuration

Create `config/default.toml` in the working directory:

```toml
[security]
deny_by_default = true

[provider]
primary = "anthropic"           # anthropic, openai, deepseek, groq, google, ollama
model = "claude-sonnet-4-20250514"
max_tokens = 1024
temperature = 0.3
fallback = ["ollama"]           # fallback chain (tried in order)

# Domain-specific config goes in your application, not here

[provider.keys]
# Per-provider API keys (override primary api_key)
# openai = "sk-..."
# google = "AIza..."
```

Set `ONECLAW_API_KEY` or provider-specific env vars (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GOOGLE_API_KEY`, etc.).

## Deployment (Edge Devices)

### Cross-Compile

```bash
# Option A: Using cross (Docker-based, recommended)
cargo install cross --git https://github.com/cross-rs/cross
./scripts/cross-build.sh 1.5.1

# Option B: Manual (requires ARM cross-compiler installed)
rustup target add aarch64-unknown-linux-gnu
cargo build --release --target aarch64-unknown-linux-gnu
```

### Install on Raspberry Pi

```bash
# Copy binary + deploy files to Pi
scp target/release/oneclaw-core pi@raspberrypi:~/
scp deploy/oneclaw.service deploy/install.sh pi@raspberrypi:~/

# SSH to Pi and install
ssh pi@raspberrypi
sudo ./install.sh 1.5.1
sudo nano /opt/oneclaw/config/default.toml  # Edit config
sudo systemctl start oneclaw
journalctl -u oneclaw -f  # Watch logs
```

### systemd Commands

```bash
sudo systemctl start oneclaw    # Start
sudo systemctl stop oneclaw     # Stop
sudo systemctl restart oneclaw  # Restart
sudo systemctl status oneclaw   # Status
sudo systemctl enable oneclaw   # Start on boot
journalctl -u oneclaw -f        # Live logs
```

### Uninstall

```bash
sudo ./deploy/uninstall.sh
```

## Project Structure

```
oneclaw/
├── crates/
│   ├── oneclaw-core/       # Runtime, traits, registry (all 6 layers)
│   ├── oneclaw-providers/  # LLM providers (Ollama, OpenAI-compat)
│   ├── oneclaw-tools/      # Built-in tools (system_info, file_write, notify)
│   └── oneclaw-channels/   # Channel implementations (CLI, TCP, Telegram, MQTT)
├── deploy/
│   ├── oneclaw.service     # systemd unit file
│   ├── install.sh          # Edge device installer
│   └── uninstall.sh        # Clean removal
├── scripts/
│   ├── gate-check.sh       # Sprint gate validation
│   ├── cross-build.sh      # ARM cross-compilation
│   └── bench.sh            # Benchmark suite runner
└── README.md
```

## Commands

| Command | Description |
|---------|-------------|
| `help` | Show all commands |
| `status` | System overview with metrics |
| `health` | Detailed layer health check |
| `metrics` | Full operational telemetry |
| `pair` / `verify CODE` | Device pairing (security) |
| `remember <text>` | Store in memory |
| `recall <query>` | Search memory (FTS5) |
| `ask <question>` | Ask AI (single LLM call) |
| `providers` | List LLM providers and status |
| `events` | Show event bus state |
| `tools` | List available tools |
| `tool <name> [key=val]` | Execute a tool |
| `channels` | List active channels |
| `reload` | Check config changes |
| `exit` / `quit` | Graceful shutdown |

## Development

```bash
# Run all tests
cargo test --workspace

# Clippy lint (zero warnings policy)
cargo clippy --workspace -- -D warnings

# Benchmarks
./scripts/bench.sh

# Gate check (sprint validation)
./scripts/gate-check.sh
```

## Design Principles

1. **Trait-driven** — Every layer is a trait. Swap Noop <-> Default <-> Custom.
2. **Deny-by-default** — Security blocks everything unless explicitly allowed.
3. **Graceful degradation** — LLM offline? Falls back to noop. Memory full? Handles gracefully.
4. **Domain-agnostic** — Kernel knows nothing about your domain. Your app adds the domain logic.
5. **Edge-viable** — Tokio async runtime, no garbage collector, ~3.4MB binary, ARM cross-compile ready.

## License

Dual-licensed under MIT and Apache 2.0.
