//! OneClaw Benchmark Suite
//! Validates edge/IoT performance targets.
//!
//! Targets:
//! - Boot time: <10ms (cold start to ready)
//! - Message throughput: >1000 msg/sec (simple commands)
//! - Memory search: <5ms per query
//! - Event bus: >5000 events/sec (publish + drain)
//! - Router analysis: <100μs
//! - Tool execution: <100μs
//! - Security check: <10μs
//! - Binary: <5MB
//!
//! Run: cargo test --release -p oneclaw-core --test bench_suite -- --nocapture --test-threads=1

use async_trait::async_trait;
use oneclaw_core::config::OneClawConfig;
use oneclaw_core::runtime::Runtime;
use oneclaw_core::memory::{MemoryMeta, MemoryQuery};
use oneclaw_core::event_bus::Event;
use oneclaw_core::channel::{Channel, IncomingMessage, OutgoingMessage};
use std::time::{Duration, Instant};
use std::sync::Mutex;

// ==================== HELPERS ====================

struct BenchChannel {
    inputs: Mutex<Vec<String>>,
}
impl BenchChannel {
    fn new(inputs: Vec<String>) -> Self {
        Self { inputs: Mutex::new(inputs) }
    }
}
#[async_trait]
impl Channel for BenchChannel {
    fn name(&self) -> &str { "bench" }
    async fn receive(&self) -> oneclaw_core::error::Result<Option<IncomingMessage>> {
        let mut i = self.inputs.lock().unwrap();
        match i.pop() {
            Some(c) => Ok(Some(IncomingMessage {
                source: "bench".into(),
                content: c,
                timestamp: chrono::Utc::now(),
            })),
            None => Ok(Some(IncomingMessage {
                source: "bench".into(),
                content: "exit".into(),
                timestamp: chrono::Utc::now(),
            })),
        }
    }
    async fn send(&self, _msg: &OutgoingMessage) -> oneclaw_core::error::Result<()> { Ok(()) }
}

fn bench<F: FnMut()>(name: &str, iterations: usize, mut f: F) -> Duration {
    // Warmup
    for _ in 0..3 { f(); }

    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    let total = start.elapsed();
    let per_iter = total / iterations as u32;

    println!(
        "  {:<40} {:>8} iters | {:>10} total | {:>8}/iter",
        name,
        iterations,
        format!("{:.2?}", total),
        format!("{:.2?}", per_iter),
    );

    per_iter
}

// ==================== BENCHMARKS ====================

#[test]
fn bench_01_boot_time() {
    println!("\n=== BOOT TIME ===");

    let per_iter = bench("Runtime::with_defaults()", 100, || {
        let config = OneClawConfig::default_config();
        let _runtime = Runtime::with_defaults(config);
    });

    // Target: <10ms boot
    assert!(
        per_iter < Duration::from_millis(10),
        "FAIL: Boot time {:.2?} exceeds 10ms target",
        per_iter
    );
    println!("  ✅ PASS: Boot time {:.2?} < 10ms target", per_iter);
}

#[test]
fn bench_02_boot_with_config() {
    println!("\n=== BOOT WITH CONFIG ===");

    let per_iter = bench("Runtime::from_config()", 50, || {
        let config = OneClawConfig::default_config();
        let workspace = std::env::current_dir().unwrap();
        let _runtime = Runtime::from_config(config, workspace).unwrap();
    });

    // from_config includes Registry resolve + SQLite init — allow 50ms
    assert!(
        per_iter < Duration::from_millis(50),
        "FAIL: Config boot {:.2?} exceeds 50ms target",
        per_iter.as_millis()
    );
    println!("  ✅ PASS: Config boot {:.2?} < 50ms target", per_iter);
}

#[tokio::test]
async fn bench_03_message_throughput() {
    println!("\n=== MESSAGE THROUGHPUT ===");

    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // Generate N "help" messages + exit (help is always-open, no security gate)
    let n = 500;
    let mut messages: Vec<String> = (0..n).map(|_| "help".to_string()).collect();
    messages.push("exit".into());
    messages.reverse(); // BenchChannel pops from end

    let ch = BenchChannel::new(messages);

    let start = Instant::now();
    runtime.run(&ch).await.unwrap();
    let elapsed = start.elapsed();

    let msgs_per_sec = (n as f64 / elapsed.as_secs_f64()) as u64;
    println!(
        "  {:<40} {} messages in {:.2?} ({} msg/sec)",
        "process_message throughput",
        n, elapsed, msgs_per_sec
    );

    // Target: >1000 msg/sec for simple commands
    assert!(
        msgs_per_sec > 1000,
        "FAIL: Throughput {} msg/sec below 1000 target",
        msgs_per_sec
    );
    println!("  ✅ PASS: {} msg/sec > 1000 target", msgs_per_sec);
}

#[test]
fn bench_04_memory_store() {
    println!("\n=== MEMORY STORE ===");

    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    let per_iter = bench("memory.store()", 500, || {
        let _ = runtime.memory.store(
            "sensor_01 | temperature | value = 22.5",
            MemoryMeta::default(),
        );
    });

    // Target: <1ms per store (NoopMemory is instant, SQLite <1ms)
    assert!(
        per_iter < Duration::from_millis(1),
        "FAIL: Memory store {:.2?} exceeds 1ms target",
        per_iter
    );
    println!("  ✅ PASS: Memory store {:.2?} < 1ms target", per_iter);
}

#[test]
fn bench_05_memory_search() {
    println!("\n=== MEMORY SEARCH ===");

    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // Pre-populate memory
    for i in 0..100 {
        let _ = runtime.memory.store(
            &format!("Device {} | value = {}", i, 20 + i),
            MemoryMeta::default(),
        );
    }

    let per_iter = bench("memory.search()", 500, || {
        let query = MemoryQuery::new("Device value").with_limit(10);
        let _ = runtime.memory.search(&query);
    });

    // Target: <5ms per search
    assert!(
        per_iter < Duration::from_millis(5),
        "FAIL: Memory search {:.2?} exceeds 5ms target",
        per_iter
    );
    println!("  ✅ PASS: Memory search {:.2?} < 5ms target", per_iter);
}

#[test]
fn bench_06_event_bus_throughput() {
    println!("\n=== EVENT BUS THROUGHPUT ===");

    use oneclaw_core::event_bus::{EventBus, DefaultEventBus};
    let bus = DefaultEventBus::new();

    let n = 1000;
    let start = Instant::now();
    for i in 0..n {
        let event = Event::new(format!("bench.{}", i % 10), "bench");
        let _ = bus.publish(event);
    }
    let drained = bus.drain().unwrap_or(0);
    let elapsed = start.elapsed();

    let events_per_sec = (n as f64 / elapsed.as_secs_f64()) as u64;
    println!(
        "  {:<40} {} events in {:.2?} ({} evt/sec, {} drained)",
        "publish + drain throughput",
        n, elapsed, events_per_sec, drained
    );

    // Target: >5000 events/sec
    assert!(
        events_per_sec > 5000,
        "FAIL: Event throughput {} evt/sec below 5000 target",
        events_per_sec
    );
    println!("  ✅ PASS: {} evt/sec > 5000 target", events_per_sec);
}

#[test]
fn bench_07_router_complexity_analysis() {
    println!("\n=== ROUTER ANALYSIS ===");

    use oneclaw_core::orchestrator::router::analyze_complexity;

    let messages = vec![
        "hello",
        "analyze trend data from sensor Alpha over 7 days",
        "emergency critical system failure detected!",
        "What is the weather today?",
        "explain the sensor readings from this morning",
    ];

    let per_iter = bench("analyze_complexity() x5", 10_000, || {
        for msg in &messages {
            let _ = analyze_complexity(msg, true);
        }
    });

    // Target: <100μs per batch of 5 analyses
    assert!(
        per_iter < Duration::from_micros(100),
        "FAIL: Router analysis {:.2?} exceeds 100μs target",
        per_iter
    );
    println!("  ✅ PASS: Router analysis {:.2?} < 100μs target", per_iter);
}

#[test]
fn bench_08_binary_size() {
    println!("\n=== BINARY SIZE ===");

    // Navigate from crate root to workspace target/
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    // Check workspace target for any release binary
    let binary = manifest_dir.join("../../target/release/oneclaw-core");
    if binary.exists() {
        let size = std::fs::metadata(binary).unwrap().len();
        let size_mb = size as f64 / (1024.0 * 1024.0);
        println!("  Binary: {:.2} MB ({} bytes)", size_mb, size);

        assert!(
            size < 5 * 1024 * 1024,
            "FAIL: Binary {:.2}MB exceeds 5MB target",
            size_mb
        );
        println!("  ✅ PASS: {:.2}MB < 5MB target", size_mb);
    } else {
        println!("  ⚠ Release binary not found — run: cargo build --release");
        println!("  (Skipping binary size check)");
    }
}

#[test]
fn bench_09_tool_execution() {
    println!("\n=== TOOL EXECUTION ===");

    use oneclaw_core::tool::{NoopTool, ToolRegistry};
    use std::collections::HashMap;

    let mut reg = ToolRegistry::new();
    reg.register(Box::new(NoopTool::new()));

    let per_iter = bench("tool_registry.execute(noop)", 5000, || {
        let _ = reg.execute("noop", &HashMap::new(), None);
    });

    // Target: <100μs per tool execution
    assert!(
        per_iter < Duration::from_micros(100),
        "FAIL: Tool execution {:.2?} exceeds 100μs target",
        per_iter
    );
    println!("  ✅ PASS: Tool execution {:.2?} < 100μs target", per_iter);
}

#[test]
fn bench_10_security_check() {
    println!("\n=== SECURITY CHECK ===");

    use oneclaw_core::security::{SecurityCore, DefaultSecurity, Action, ActionKind};

    let workspace = std::env::current_dir().unwrap();
    // Development mode: pairing_required=false (so authorize doesn't reject)
    let security = DefaultSecurity::development(&workspace);

    let action = Action {
        kind: ActionKind::Execute,
        resource: "command".into(),
        actor: "bench-device".into(),
    };

    let per_iter = bench("security.authorize()", 10_000, || {
        let _ = security.authorize(&action);
    });

    // Target: <10μs per security check
    assert!(
        per_iter < Duration::from_micros(10),
        "FAIL: Security check {:.2?} exceeds 10μs target",
        per_iter
    );
    println!("  ✅ PASS: Security check {:.2?} < 10μs target", per_iter);
}
