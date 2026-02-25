# BLUEPRINT: OneClaw
## Systems Infrastructure (Rust AI Agent Core) — Vibecode Kit v5.0

---

### PROJECT INFO
| Field | Value |
|-------|-------|
| Dự án | **OneClaw** (codename: PULSE) |
| Loại | Systems Infrastructure — Rust AI Agent Kernel + IoT Verticals |
| Ngày | 21/02/2026 |
| Chủ thầu | Claude Opus — Chief Architect |
| Thợ | Claude Code — Builder |
| Chủ nhà | Quỳnh — Technical Project Manager & Architect |

---

### MỤC TIÊU

**Primary Goal:** Xây dựng một AI Agent Kernel viết bằng Rust, siêu nhẹ (<5MB RAM), chạy edge/IoT, với LLM Orchestration vượt trội mà không dự án nào có.

**Competitive Moat:** LLM Orchestration Engine — 4-factor smart routing + multi-step chain executor + graceful degradation (hoạt động cả khi offline). OpenClaw và ZeroClaw đều coi LLM là black box. OneClaw coi LLM là đội ngũ chuyên gia cần điều phối.

**Reference Vertical:** Elderly Care Agent — stress-test core bằng use case cần tất cả 5 layers.

**Chiến lược:** Concurrent Core + Vertical — mỗi sprint ship cả core trait lẫn vertical implementation.

---

### QUYẾT ĐỊNH KỸ THUẬT ĐÃ DUYỆT

| # | Quyết định | Lựa chọn | Lý do |
|---|-----------|----------|-------|
| D-001 | Rust Edition | **2024** | Mới nhất, tận dụng async trait native |
| D-002 | Default Providers | **Ollama + OpenAI-compatible song song** | Cover cả local + cloud từ đầu |
| D-003 | License | **Dual MIT/Apache 2.0** | Chuẩn Rust ecosystem |
| D-004 | Target Hardware | ARM64 (RPi4+), x86_64, RISC-V | 99% IoT hardware |
| D-005 | Database | SQLite (rusqlite) + FTS5 | Zero-config, embedded |
| D-006 | Async Runtime | Tokio | Mature, lightweight |
| D-007 | Config Format | TOML | Rust standard |
| D-008 | IoT Protocol | MQTT (rumqttc) | Industry standard |
| D-009 | Encryption | ring (AES-256-GCM) | Audited, no OpenSSL |
| D-010 | Architecture | 5-layer trait-driven | Đã validate qua phân tích OpenClaw/ZeroClaw |

---

### KIẾN TRÚC 5 TẦNG

```
┌─────────────────────────────────────────────────────────┐
│              OneClaw Core Architecture                   │
├─────────────────────────────────────────────────────────┤
│  Layer 5: CHANNEL TRAIT (I/O Interface)                  │
│  └─ CLI | Telegram | MQTT | Custom Webhook              │
├─────────────────────────────────────────────────────────┤
│  Layer 4: TOOL TRAIT (Actions)                           │
│  └─ Shell | File | GPIO | HTTP | Custom                 │
├─────────────────────────────────────────────────────────┤
│  Layer 3: EVENT BUS (Nervous System) ★ NEW              │
│  └─ Pub/Sub | Sensor Stream | Alert Pipeline            │
├─────────────────────────────────────────────────────────┤
│  Layer 2: MEMORY TRAIT (Brain)                           │
│  └─ Tri-Search (Vector+FTS5+Temporal) | Context Window  │
├─────────────────────────────────────────────────────────┤
│  Layer 1: LLM ORCHESTRATOR (Heart) ★ MOAT               │
│  └─ Router | Chain | Context Mgr | Fallback             │
├─────────────────────────────────────────────────────────┤
│  Layer 0: SECURITY CORE (Immune System)                  │
│  └─ Deny-default | Encrypt | Pair | Scope               │
└─────────────────────────────────────────────────────────┘
  Runtime: Rust 2024 native binary | <5MB RAM | <10ms boot
```

---

### PROJECT STRUCTURE

```
oneclaw/
├── Cargo.toml                  # Workspace root
├── LICENSE-MIT
├── LICENSE-APACHE
├── README.md
├── rust-toolchain.toml         # Pin Rust 2024 edition
├── .cargo/
│   └── config.toml             # Cross-compile targets
├── crates/
│   ├── oneclaw-core/           # THE KERNEL (< 2MB)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs          # Re-export all traits
│   │       ├── security.rs     # Layer 0
│   │       ├── orchestrator/   # Layer 1
│   │       │   ├── mod.rs
│   │       │   ├── router.rs       # ModelRouter trait
│   │       │   ├── context.rs      # ContextManager trait
│   │       │   ├── chain.rs        # ChainExecutor trait
│   │       │   └── fallback.rs     # Degradation modes
│   │       ├── memory/         # Layer 2
│   │       │   ├── mod.rs
│   │       │   ├── traits.rs       # Memory trait
│   │       │   ├── trisearch.rs    # Vector+FTS5+Temporal
│   │       │   └── sqlite.rs       # Default backend
│   │       ├── event_bus/      # Layer 3
│   │       │   ├── mod.rs
│   │       │   ├── traits.rs       # EventBus trait
│   │       │   ├── pipeline.rs     # Filter/Transform/Route
│   │       │   └── bus.rs          # Default impl (tokio broadcast)
│   │       ├── tool/           # Layer 4
│   │       │   ├── mod.rs
│   │       │   ├── traits.rs       # Tool trait
│   │       │   └── sandbox.rs      # Resource-limited execution
│   │       ├── channel/        # Layer 5
│   │       │   ├── mod.rs
│   │       │   └── traits.rs       # Channel trait
│   │       ├── config.rs       # TOML config loader
│   │       ├── error.rs        # Unified error types
│   │       └── runtime.rs      # Main event loop
│   ├── oneclaw-providers/      # LLM provider impls
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── ollama.rs
│   │       └── openai_compat.rs
│   ├── oneclaw-channels/       # Channel impls
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── cli.rs
│   │       ├── mqtt.rs
│   │       └── telegram.rs
│   ├── oneclaw-tools/          # Tool impls
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── file_ops.rs
│   │       ├── shell.rs
│   │       ├── http_client.rs
│   │       └── notify.rs
│   └── oneclaw-elderly/        # Reference Vertical
│       ├── Cargo.toml
│       ├── config/
│       │   └── default.toml
│       └── src/
│           ├── lib.rs
│           ├── main.rs
│           ├── vitals.rs       # Health memory schema
│           ├── anomaly.rs      # Detection chains
│           ├── pipelines.rs    # Sensor event pipelines
│           └── caregiver.rs    # Caregiver interaction
├── tests/                      # Integration tests
│   └── core_boot_test.rs
├── benches/                    # Benchmarks
│   └── startup_bench.rs
└── docs/
    ├── ADR/                    # Architecture Decision Records
    │   └── 001-five-layer-architecture.md
    └── CONTRIBUTING.md
```

---

### NGUYÊN TẮC BẤT KHẢ THƯƠNG LƯỢNG

1. **Core binary < 2MB.** Vượt = có thành phần không thuộc core.
2. **Mỗi tầng đúng 1 trait, đúng 1 nhiệm vụ.** Vi phạm = refactor ngay.
3. **Mọi vertical-specific code nằm NGOÀI crates/oneclaw-core/.** Không ngoại lệ.
4. **Mọi trait phải có NoopImpl.** Core chạy được mà không cần bất kỳ vertical nào.
5. **Không có tầng thứ 6.** Phải chứng minh tại sao 5 tầng không đủ.
6. **Mọi LLM call phải qua Orchestrator.** Không có shortcut.
7. **Deny-by-default security.** Mọi action phải authorize.

---

### TASK GRAPH — SPRINT 1-2 (Tuần 1-4)

```
TIP-001: Cargo Workspace Scaffold ─────────────────────┐
    │                                                    │
    ▼                                                    │
TIP-002: Error Types + Config Loader ──────────────┐    │
    │                                               │    │
    ▼                                               ▼    ▼
TIP-003: Security Core Trait       TIP-004: Trait Registry + NoopImpls
    │                                               │
    └──────────────────┬────────────────────────────┘
                       │
                       ▼
                 TIP-005: Runtime Bootstrap
                 (core boots + logs + loads config)
                       │
                       ▼
                 TIP-006: Elderly Care — Device Pairing PoC
                 (VALIDATE: Security Core end-to-end)
```

**Sprint 1-2 Gate Check:**
- `cargo build --release` → binary < 500KB
- `cargo test` → 100% pass
- `cargo clippy -- -D warnings` → 0 warnings
- `cargo bench` → startup < 5ms on x86_64
- Device pairing flow chạy được

---

### SPRINT ROADMAP TỔNG THỂ

| Sprint | Tuần | Core Deliverable | Elderly Care Deliverable | Gate |
|--------|------|-----------------|-------------------------|------|
| 1-2 | 1-4 | Scaffold + Security + Config + Runtime | Device Pairing PoC | Binary <500KB |
| 3-4 | 5-8 | Memory Trait + Tri-Search Engine | Vitals Storage + Temporal Query | "BP tuần trước?" → correct |
| 5-6 | 9-12 | LLM Orchestrator: Router + Context | Health Analyzer + Anomaly Detect | Smart model selection |
| 7-8 | 13-16 | LLM Orchestrator: Chain + Fallback | Detect→Analyze→Alert→Verify | Offline alert works |
| 9-10 | 17-20 | Event Bus + Pipeline Engine | Sensor pipeline: temp/motion/door | 1000 evt/s, <5 LLM/min |
| 11-12 | 21-24 | Tool Trait + Sandbox | GPIO + MQTT + Notify tools | Tool controls device |
| 13-14 | 25-28 | Channel: CLI + MQTT + Telegram | Caregiver Telegram bot | E2E demo |
| 15-16 | 29-32 | Observability + Benchmark + Docs | 72h stress test | Core <2MB, <5MB RAM |

---

### CHECKPOINT

Chủ nhà xác nhận:
- [ ] Kiến trúc 5 tầng đúng mong muốn
- [ ] Project structure hợp lý
- [ ] 7 nguyên tắc bất khả thương lượng OK
- [ ] Sprint roadmap khả thi
- [ ] Task Graph Sprint 1-2 hợp lý
- [ ] Không thiếu gì quan trọng

**Reply "APPROVED" để nhận TIP-001 cho Thợ Claude Code.**
