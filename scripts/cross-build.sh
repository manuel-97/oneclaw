#!/bin/bash
# OneClaw Cross-Build Script
# Builds release binaries for ARM targets + host
#
# Usage: ./scripts/cross-build.sh [VERSION]
#
# Prerequisites:
#   Option A (recommended): cargo install cross --git https://github.com/cross-rs/cross
#   Option B (manual):      Install aarch64-linux-gnu-gcc / arm-linux-gnueabihf-gcc
#
# Dependencies verified for cross-compile:
#   - rusqlite: bundled feature (compiles SQLite from C source)
#   - reqwest: rustls-tls (pure Rust TLS, no system OpenSSL)

set -e

VERSION="${1:-1.2.0}"

echo "======================================================="
echo "  OneClaw Cross-Build v${VERSION}"
echo "======================================================="
echo ""

# Detect available tools
if command -v cross &> /dev/null; then
    BUILD_CMD="cross"
    echo "  Using: cross (Docker-based)"
elif command -v cargo &> /dev/null; then
    BUILD_CMD="cargo"
    echo "  Using: cargo (manual cross-compile)"
    echo "  Note: Ensure cross-compiler toolchains are installed"
else
    echo "  ERROR: Neither cross nor cargo found!"
    exit 1
fi

TARGETS=(
    "aarch64-unknown-linux-gnu"      # RPi 4/5 64-bit, Orange Pi, NanoPi
    "armv7-unknown-linux-gnueabihf"  # RPi 3/Zero 32-bit, older ARM
)

mkdir -p release

BUILT=0
FAILED=0

for TARGET in "${TARGETS[@]}"; do
    echo ""
    echo ">> Building for ${TARGET}..."

    # Add target if using cargo directly
    if [ "$BUILD_CMD" = "cargo" ]; then
        rustup target add "$TARGET" 2>/dev/null || true
    fi

    if $BUILD_CMD build --release -p oneclaw-elderly --target "$TARGET" 2>&1; then
        BINARY="target/${TARGET}/release/oneclaw-elderly"
        if [ -f "$BINARY" ]; then
            SIZE=$(ls -la "$BINARY" | awk '{print $5}')
            SIZE_KB=$((SIZE / 1024))
            echo "  PASS ${TARGET}: ${SIZE_KB} KB"
            cp "$BINARY" "release/oneclaw-elderly-${VERSION}-${TARGET}"
            BUILT=$((BUILT + 1))
        else
            echo "  WARN Binary not found at ${BINARY}"
            FAILED=$((FAILED + 1))
        fi
    else
        echo "  SKIP Build failed for ${TARGET} (cross-compiler may not be installed)"
        FAILED=$((FAILED + 1))
    fi
done

# Always build for host
echo ""
echo ">> Building for host..."
cargo build --release -p oneclaw-elderly
HOST_BINARY="target/release/oneclaw-elderly"
HOST_SIZE=$(ls -la "$HOST_BINARY" | awk '{print $5}')
HOST_KB=$((HOST_SIZE / 1024))
echo "  PASS host: ${HOST_KB} KB"
cp "$HOST_BINARY" "release/oneclaw-elderly-${VERSION}-host"
BUILT=$((BUILT + 1))

echo ""
echo "======================================================="
echo "  Cross-build complete: ${BUILT} built, ${FAILED} skipped"
echo "======================================================="
echo ""
echo "  Binaries:"
ls -lh release/oneclaw-elderly-${VERSION}-* 2>/dev/null || echo "  (none)"
echo ""
echo "  Deploy to Pi:"
echo "    scp release/oneclaw-elderly-${VERSION}-aarch64-unknown-linux-gnu pi@raspberrypi:~/"
echo "    scp deploy/oneclaw.service deploy/install.sh pi@raspberrypi:~/"
