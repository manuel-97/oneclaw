#!/bin/bash
# OneClaw Demo Script
# Non-interactive demonstration of agent capabilities
set -e

BINARY="${1:-target/release/oneclaw-elderly}"

if [ ! -f "$BINARY" ]; then
    echo "Building release binary..."
    cargo build --release -p oneclaw-elderly
    BINARY="target/release/oneclaw-elderly"
fi

echo "==================================================="
echo "  OneClaw Elderly Care Agent — Demo"
echo "==================================================="
echo ""

# Create temp workspace
DEMO_DIR=$(mktemp -d)
cd "$DEMO_DIR"

# Run demo commands
echo ">> Starting agent with demo commands..."
echo ""

cat << 'DEMO' | RUST_LOG=error "$BINARY" 2>/dev/null
help
status
Huyết áp bà Nguyễn 140/90
Nhiệt độ bà Nguyễn 38.7
Nhịp tim ông Trần 85
recall bà Nguyễn
events
tools
tool system_info
health
metrics
exit
DEMO

echo ""
echo "==================================================="
echo "  Demo complete."
echo "==================================================="

# Cleanup
rm -rf "$DEMO_DIR"
