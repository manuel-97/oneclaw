# Vibecode Kit v5.0 — Task Instruction Pack

## VAI TRÒ
Bạn là THỢ THI CÔNG trong hệ thống Vibecode Kit v5.0.

## QUY TẮC TUYỆT ĐỐI
1. IMPLEMENT ĐÚNG TIP specification bên dưới
2. SELF-TEST theo acceptance criteria
3. BÁO CÁO theo Completion Report format
4. Gặp conflict → BÁO CÁO, KHÔNG tự quyết định

## PROJECT CONTEXT
- **Project:** OneClaw — Rust AI Agent Kernel cho Edge/IoT
- **Working directory:** ~/oneclaw/
- **Trạng thái:** v1.5.0 Smart Providers — SHIPPED. 532 tests, 3.4MB, 0 clippy, 0 production unwrap
- **Mục tiêu:** X-ray TOÀN BỘ codebase, xuất bản Handover Document đầy đủ để Thầu mới (cửa sổ chat mới) có thể fork sang dự án oneclaw-home mà KHÔNG cần hỏi lại bất kỳ điều gì về kernel.

---

# TIP-XRAY: Codebase X-Ray & Handover Document

## HEADER
- **TIP-ID:** TIP-XRAY
- **Project:** OneClaw
- **Module:** ALL — toàn bộ workspace
- **Priority:** P0
- **Estimated effort:** 30-45 phút

## CONTEXT

**Problem:** OneClaw v1.5.0 đã ship. Bây giờ cần tách ra dự án mới (oneclaw-home) để phát triển Smart Home pipeline. Thầu mới sẽ làm việc trong cửa sổ chat mới, KHÔNG có context từ 36 TIPs trước. Cần bản handover đủ chi tiết để thầu mới hiểu kernel 100% mà không cần đọc source code.

**Solution:** Scan toàn bộ codebase, xuất 1 file markdown duy nhất chứa tất cả thông tin kiến trúc, API, cấu trúc, conventions, dependencies, và hướng dẫn fork.

## SPECIFICATIONS

### OUTPUT: Tạo file `ONECLAW-KERNEL-HANDOVER.md` chứa đầy đủ các phần sau:

---

### PHẦN 1: PROJECT OVERVIEW

```bash
# Chạy và ghi lại:
echo "=== PROJECT STATS ==="
find ~/oneclaw -name "*.rs" | wc -l                    # Số file Rust
find ~/oneclaw -name "*.rs" -exec cat {} + | wc -l     # Tổng dòng code
cargo test --workspace 2>&1 | tail -5                    # Test count
cargo build --release 2>&1 | tail -3                     # Build status
ls -lh target/release/oneclaw*                           # Binary size
cargo clippy --workspace 2>&1 | grep -c "warning"       # Clippy warnings
```

Ghi vào handover:
- Tên dự án, version, mô tả ngắn
- Ngôn ngữ, toolchain version (rustc --version)
- Số file, dòng code, tests, binary size
- Trạng thái: stable / in-progress

---

### PHẦN 2: WORKSPACE STRUCTURE

```bash
# Chạy:
tree ~/oneclaw -L 3 -I "target|.git" --dirsfirst
# HOẶC nếu không có tree:
find ~/oneclaw -type f -name "*.rs" -o -name "*.toml" -o -name "*.md" | sort
```

Ghi vào handover:
- Cấu trúc thư mục đầy đủ (tree output)
- Giải thích mỗi crate/module: tên, vai trò, phụ thuộc
- File nào là entry point (main.rs, lib.rs)

---

### PHẦN 3: CARGO WORKSPACE & DEPENDENCIES

```bash
# Chạy:
cat ~/oneclaw/Cargo.toml                                 # Root workspace
find ~/oneclaw/crates -name "Cargo.toml" -exec echo "=== {} ===" \; -exec cat {} \;
cargo tree --depth 2                                     # Dependency tree
```

Ghi vào handover:
- Workspace members
- Mỗi crate: dependencies (tên, version, features)
- [profile.release] settings
- Feature flags nếu có

---

### PHẦN 4: ARCHITECTURE — 6 LAYERS

Với MỖI layer, scan source code và ghi lại:

#### 4.1 Security Layer
```bash
grep -rn "pub fn\|pub struct\|pub enum\|pub trait\|pub type" crates/*/src/security* --include="*.rs"
```
- Structs & traits
- Public API (function signatures đầy đủ)
- HMAC-SHA256 implementation details
- Device pairing flow

#### 4.2 Memory Layer
```bash
grep -rn "pub fn\|pub struct\|pub enum\|pub trait\|pub type" crates/*/src/memory* --include="*.rs"
```
- SQLite schema (tất cả CREATE TABLE statements)
- FTS5 configuration
- Public API: store, recall, search
- Memory types (vitals, conversations, etc.)

#### 4.3 NLP Layer
```bash
grep -rn "pub fn\|pub struct\|pub enum\|pub trait\|pub type" crates/*/src/nlp* --include="*.rs"
```
- NLP pipeline stages
- Intent/entity extraction
- Vietnamese language handling

#### 4.4 Pipeline Layer
```bash
grep -rn "pub fn\|pub struct\|pub enum\|pub trait\|pub type" crates/*/src/pipeline* crates/*/src/chain* crates/*/src/bus* --include="*.rs"
```
- Event Bus: events, subscribers, publish/subscribe API
- Pipeline Engine: stages, execution flow
- Chain Executor: multi-step reasoning

#### 4.5 Tool Layer
```bash
grep -rn "pub fn\|pub struct\|pub enum\|pub trait\|pub type" crates/*/src/tool* --include="*.rs"
```
- Tool trait definition
- Built-in tools
- Sandbox execution model
- Tool registration

#### 4.6 Channel Layer
```bash
grep -rn "pub fn\|pub struct\|pub enum\|pub trait\|pub type" crates/*/src/channel* --include="*.rs"
```
- Channel trait
- CLI, TCP, MQTT, Telegram, Sensor implementations
- Channel Router
- Alert dispatch

---

### PHẦN 5: PROVIDER SYSTEM (v1.5 — QUAN TRỌNG NHẤT)

```bash
# Scan toàn bộ provider module:
find ~/oneclaw -path "*/provider*" -name "*.rs" -exec echo "=== {} ===" \; -exec head -50 {} \;
```

Ghi chi tiết:
- **Provider trait:** Đầy đủ trait definition (copy nguyên văn)
- **ProviderConfig struct:** Tất cả fields
- **ProviderResponse struct:** Tất cả fields
- **ChatMessage & MessageRole:** Enums đầy đủ
- **TokenUsage struct**
- **6 Providers:** Với mỗi provider:
  - File path
  - Struct name
  - API endpoint + format
  - Authentication method (header name, key resolution)
  - Default model
  - Đặc biệt gì (VD: Ollama không cần key, Gemini dùng query param)
- **ReliableProvider:** Retry logic, wrapping API
- **FallbackChain:** Chain construction, failover logic, provider_info()
- **build_provider_chain():** Function signature + logic flow
- **Config TOML format:** Ví dụ đầy đủ [provider] section

---

### PHẦN 6: RUNTIME & CONFIG

```bash
grep -rn "pub fn\|pub struct" crates/*/src/runtime* crates/*/src/config* crates/*/src/registry* --include="*.rs"
cat ~/oneclaw/oneclaw.toml 2>/dev/null || echo "No config file found"
find ~/oneclaw -name "*.toml" -not -name "Cargo.toml" -not -path "*/target/*"
```

Ghi vào handover:
- Runtime struct: fields, lifecycle (init → run → shutdown)
- Config parsing: TOML structure đầy đủ
- Registry: component registration flow
- CLI commands available

---

### PHẦN 7: ERROR HANDLING

```bash
grep -rn "pub enum.*Error\|OneClawError" crates/ --include="*.rs" | head -30
# Tìm error module:
find ~/oneclaw -name "error.rs" -exec cat {} \;
```

Ghi vào handover:
- OneClawError enum variants đầy đủ
- Result type alias
- Error propagation pattern (? operator usage)

---

### PHẦN 8: TESTING PATTERNS

```bash
# Đếm tests per module:
cargo test --workspace -- --list 2>&1 | grep "::" | sed 's/::.*//' | sort | uniq -c | sort -rn

# Test file locations:
find ~/oneclaw -name "*test*" -name "*.rs" | sort
```

Ghi vào handover:
- Test count per module
- Test naming convention
- Integration test locations
- Live test pattern (#[ignore] + env var)
- How to run: cargo test commands

---

### PHẦN 9: CODE CONVENTIONS

Scan codebase và tổng kết:

```bash
# Logging pattern:
grep -rn "tracing::\|info!\|warn!\|debug!\|error!" crates/ --include="*.rs" | head -20

# Error pattern:
grep -rn "OneClawError::" crates/ --include="*.rs" | head -20

# Module pattern:
find ~/oneclaw/crates -name "mod.rs" -exec echo "=== {} ===" \; -exec head -20 {} \;
```

Ghi vào handover:
- Import ordering convention (std → external → crate → super)
- Logging: tracing crate, structured logging
- Error: OneClawError::Variant(message)
- Module: mod.rs with pub use re-exports
- Naming: snake_case files, CamelCase structs
- Doc comments: //! for modules, /// for items
- No unwrap() in production code
- No println!() — use tracing

---

### PHẦN 10: HƯỚNG DẪN FORK CHO ONECLAW-HOME

Viết hướng dẫn cụ thể:

```markdown
## Cách tạo oneclaw-home từ oneclaw kernel

### Bước 1: Tạo project mới
cargo new oneclaw-home
cd oneclaw-home

### Bước 2: Thêm dependency vào Cargo.toml
[dependencies]
oneclaw-core = { path = "../oneclaw/crates/oneclaw-core" }
# HOẶC nếu publish crate:
# oneclaw-core = "1.5.0"

### Bước 3: Import kernel components
use oneclaw_core::provider::{Provider, FallbackChain, build_provider_chain};
use oneclaw_core::pipeline::{Pipeline, Event};
use oneclaw_core::channel::mqtt::MqttChannel;
use oneclaw_core::memory::Memory;
use oneclaw_core::tool::{Tool, ToolRegistry};
use oneclaw_core::security::Security;
use oneclaw_core::config::Config;
use oneclaw_core::runtime::Runtime;

### Bước 4: Extend — KHÔNG modify kernel
- Tạo Device Registry (mới)
- Tạo Scene Engine (mới)
- Tạo Voice Interface (mới)
- Tạo Info Aggregator (mới)
- Wire vào kernel Pipeline + Tool Layer

### Bước 5: Build & Test
cargo build
cargo test
```

**QUAN TRỌNG:** Liệt kê tất cả public API mà oneclaw-home SẼ CẦN sử dụng. Đánh dấu API nào stable, API nào có thể thay đổi.

---

### PHẦN 11: KNOWN LIMITATIONS & TECH DEBT

```bash
# Scan remaining TODOs:
grep -rn "TODO\|FIXME\|ISSUE:\|HACK\|XXX" crates/ --include="*.rs"

# Check for any todo!() macros:
grep -rn "todo!()\|unimplemented!()" crates/ --include="*.rs"
```

Ghi vào handover:
- Danh sách TODOs/issues còn lại
- Architectural limitations (sync only, no streaming, etc.)
- Performance bottlenecks known
- Things the next developer should know

---

## ACCEPTANCE CRITERIA

```gherkin
Given: TIP-XRAY chạy xong
Then: File ONECLAW-KERNEL-HANDOVER.md tồn tại ở ~/oneclaw/docs/

Given: Một developer MỚI đọc handover
Then: Có thể hiểu:
  - Cấu trúc workspace (không cần tree)
  - 6 layers và vai trò (không cần đọc source)
  - Provider system đầy đủ (trait + 6 providers + fallback)
  - Cách tạo project mới depend on oneclaw-core
  - Public API cần dùng
  - Code conventions để follow
  - Known limitations

Given: Developer tạo oneclaw-home
Then: Có thể:
  - cargo new + add dependency
  - Import đúng modules
  - Extend kernel (không modify)
  - Build thành công
```

## CONSTRAINTS

- ❌ KHÔNG thay đổi bất kỳ source code nào
- ❌ KHÔNG thêm/xóa file trong codebase
- ✅ CHỈ đọc và xuất handover document
- ✅ Copy NGUYÊN VĂN trait definitions, struct definitions, enum definitions
- ✅ Bao gồm function signatures đầy đủ (params + return types)
- ✅ File output: `~/oneclaw/docs/ONECLAW-KERNEL-HANDOVER.md`
- ✅ Ngôn ngữ: Tiếng Anh cho code/API, Tiếng Việt cho giải thích

## WHAT NOT TO DO
- ❌ KHÔNG tóm tắt quá ngắn — cần ĐẦY ĐỦ chi tiết
- ❌ KHÔNG bỏ qua bất kỳ public API nào
- ❌ KHÔNG đoán — nếu không chắc, grep source code
- ❌ KHÔNG modify codebase
- ❌ KHÔNG chạy tests (chỉ đọc)

## REPORT FORMAT

```markdown
### COMPLETION REPORT — TIP-XRAY

**STATUS:** DONE / PARTIAL / BLOCKED

**HANDOVER FILE:** ~/oneclaw/docs/ONECLAW-KERNEL-HANDOVER.md

**COVERAGE:**
- Project overview: [YES/NO]
- Workspace structure: [YES/NO]
- Dependencies: [YES/NO]
- 6 Layers documented: [YES/NO] — [count]/6
- Provider system (full): [YES/NO]
- Runtime & Config: [YES/NO]
- Error handling: [YES/NO]
- Testing patterns: [YES/NO]
- Code conventions: [YES/NO]
- Fork guide: [YES/NO]
- Known limitations: [YES/NO]

**STATS:**
- Handover file size: [X] KB
- Public APIs documented: [count]
- Structs/Traits/Enums documented: [count]

**NOTES:**
- [anything unusual found during scan]
```
