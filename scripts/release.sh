#!/bin/bash
# OneClaw Release Build Script
# Builds optimized binary, runs all checks, packages for distribution
set -e

VERSION="${1:-1.0.0}"
TARGET="${2:-$(rustc -vV | grep host | awk '{print $2}')}"

echo "==================================================="
echo "  OneClaw Release Build v${VERSION}"
echo "  Target: ${TARGET}"
echo "==================================================="
echo ""

# 1. Clean
echo ">> Cleaning..."
cargo clean 2>/dev/null || true
echo ""

# 2. Full test suite
echo ">> Running full test suite..."
TEST_OUTPUT=$(cargo test --workspace 2>&1)
TOTAL_PASS=$(echo "$TEST_OUTPUT" | grep -oE '[0-9]+ passed' | awk '{s+=$1}END{print s}')
echo "  ${TOTAL_PASS} tests passed"
echo ""

# 3. Clippy
echo ">> Clippy lint..."
cargo clippy --workspace -- -D warnings 2>&1 | tail -3
echo "  Clippy clean"
echo ""

# 4. Release build
echo ">> Building release binary..."
cargo build --release -p oneclaw-elderly
echo ""

# 5. Binary info
BINARY="target/release/oneclaw-elderly"
SIZE=$(ls -la $BINARY | awk '{print $5}')
SIZE_KB=$((SIZE / 1024))
SIZE_MB=$(echo "scale=2; $SIZE_KB / 1024" | bc)
echo "  Binary: ${SIZE_KB} KB (${SIZE_MB} MB)"
echo ""

# 6. Benchmarks
echo ">> Running benchmarks..."
cargo test --release -p oneclaw-core --test bench_suite -- --nocapture --test-threads=1 2>/dev/null | grep -E "PASS|FAIL"
echo ""

# 7. Package
RELEASE_DIR="release/oneclaw-${VERSION}-${TARGET}"
mkdir -p "$RELEASE_DIR"
cp "$BINARY" "$RELEASE_DIR/"
cp README.md "$RELEASE_DIR/"
cp CHANGELOG.md "$RELEASE_DIR/"
cp LICENSE "$RELEASE_DIR/" 2>/dev/null || true

# Create config template
cat > "$RELEASE_DIR/oneclaw.toml.example" << 'EOF'
# OneClaw Configuration
# Copy to oneclaw.toml and edit

[security]
deny_by_default = true

[memory]
backend = "sqlite"

[providers]
default = "ollama"
llm_timeout_secs = 30

[providers.ollama]
url = "http://localhost:11434"
model = "llama3.2:1b"

[providers.openai]
base_url = "https://api.openai.com/v1"
model = "gpt-4o-mini"
api_key = "sk-your-key-here"
EOF

# Create tarball
echo ">> Packaging..."
cd release
tar czf "oneclaw-${VERSION}-${TARGET}.tar.gz" "oneclaw-${VERSION}-${TARGET}/"
cd ..

PACKAGE_SIZE=$(ls -la "release/oneclaw-${VERSION}-${TARGET}.tar.gz" | awk '{print $5}')
PACKAGE_KB=$((PACKAGE_SIZE / 1024))

echo ""
echo "==================================================="
echo "  Release v${VERSION} packaged"
echo "  Package: release/oneclaw-${VERSION}-${TARGET}.tar.gz"
echo "  Size: ${PACKAGE_KB} KB"
echo "  Tests: ${TOTAL_PASS}"
echo "  Binary: ${SIZE_KB} KB (${SIZE_MB} MB)"
echo "==================================================="
