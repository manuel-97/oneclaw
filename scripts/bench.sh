#!/bin/bash
# OneClaw Benchmark Suite v1.0
# Validates edge/IoT performance targets
set -e

echo "═══════════════════════════════════════════════════"
echo "  OneClaw Benchmark Suite v1.0"
echo "  Edge/IoT Performance Targets"
echo "═══════════════════════════════════════════════════"
echo ""
echo "Targets:"
echo "  Boot:    <10ms (cold start)"
echo "  Msg/sec: >1000 (simple commands)"
echo "  Memory:  <5ms (search)"
echo "  Events:  >5000/sec (pub+drain)"
echo "  Router:  <100μs (complexity analysis)"
echo "  Tools:   <100μs (execution)"
echo "  Security:<10μs (authorize)"
echo "  Binary:  <5MB"
echo ""

# Build release
echo "▶ Building release binary..."
cargo build --release -p oneclaw-elderly 2>&1 | tail -1
echo ""

# Run benchmark suite
echo "▶ Running benchmarks..."
echo ""
cargo test --release -p oneclaw-core --test bench_suite -- --nocapture --test-threads=1 2>/dev/null

echo ""
echo "═══════════════════════════════════════════════════"
echo "  Benchmark suite complete."
echo "═══════════════════════════════════════════════════"
