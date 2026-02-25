#!/bin/bash
# OneClaw Installation Script for Edge Devices
# Run on target device (Raspberry Pi, etc.)
#
# Usage: sudo ./install.sh [VERSION]
#
# Idempotent: safe to run multiple times. Preserves existing config.

set -e

INSTALL_DIR="/opt/oneclaw"
SERVICE_USER="oneclaw"
VERSION="${1:-1.2.0}"

echo "======================================================="
echo "  OneClaw Installer v${VERSION}"
echo "======================================================="
echo ""

# Must run as root
if [ "$EUID" -ne 0 ]; then
    echo "  ERROR: Please run with sudo"
    exit 1
fi

# Detect architecture
ARCH=$(uname -m)
case "$ARCH" in
    aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
    armv7l)  TARGET="armv7-unknown-linux-gnueabihf" ;;
    x86_64)  TARGET="host" ;;
    *)       echo "  Unsupported architecture: $ARCH"; exit 1 ;;
esac
echo "  Architecture: ${ARCH} (${TARGET})"

# Locate binary — try versioned name first, then plain
BINARY=""
for CANDIDATE in \
    "oneclaw-elderly-${VERSION}-${TARGET}" \
    "oneclaw-elderly-${TARGET}" \
    "oneclaw-elderly"; do
    if [ -f "$CANDIDATE" ]; then
        BINARY="$CANDIDATE"
        break
    fi
done

if [ -z "$BINARY" ]; then
    echo "  ERROR: Binary not found."
    echo "  Expected one of:"
    echo "    oneclaw-elderly-${VERSION}-${TARGET}"
    echo "    oneclaw-elderly"
    echo ""
    echo "  Run cross-build.sh first, then copy binary here."
    exit 1
fi
echo "  Binary: $BINARY"

# Create service user
echo ""
echo ">> Creating service user..."
if ! id "$SERVICE_USER" &>/dev/null; then
    useradd -r -s /usr/sbin/nologin -d "$INSTALL_DIR" "$SERVICE_USER"
    echo "   Created user: $SERVICE_USER"
else
    echo "   User exists: $SERVICE_USER"
fi

# Create directories
echo ">> Creating directories..."
mkdir -p "$INSTALL_DIR"
mkdir -p "$INSTALL_DIR/data"
mkdir -p "$INSTALL_DIR/config"

# Stop existing service (if running)
if systemctl is-active --quiet oneclaw 2>/dev/null; then
    echo ">> Stopping existing service..."
    systemctl stop oneclaw
fi

# Install binary
echo ">> Installing binary..."
cp "$BINARY" "$INSTALL_DIR/oneclaw-elderly"
chmod 755 "$INSTALL_DIR/oneclaw-elderly"

# Create default config (only if none exists — never overwrite user config)
if [ ! -f "$INSTALL_DIR/config/default.toml" ]; then
    echo ">> Creating default config..."
    cat > "$INSTALL_DIR/config/default.toml" << 'CONFIG'
# OneClaw Configuration
# Edit this file for your deployment

[security]
deny_by_default = true
pairing_required = true
registry_path = "data/devices.registry"

[runtime]
name = "oneclaw-elderly"
log_level = "info"

[memory]
backend = "sqlite"
db_path = "data/oneclaw.db"

[providers]
default = "ollama"

[providers.ollama]
url = "http://localhost:11434"
model = "llama3.2:1b"

# Uncomment to enable OpenAI provider:
# [providers.openai]
# base_url = "https://api.openai.com/v1"
# model = "gpt-4o-mini"
# api_key = ""

# Uncomment to enable Telegram alerts:
# [telegram]
# bot_token = "YOUR_BOT_TOKEN_FROM_BOTFATHER"
# allowed_chat_ids = [YOUR_CHAT_ID]
# polling_timeout = 30

# Uncomment to enable MQTT sensor input:
# [mqtt]
# host = "localhost"
# port = 1883
# subscribe_topics = ["sensors/#"]
# publish_prefix = "oneclaw/alerts"
CONFIG
    echo "   Config created at $INSTALL_DIR/config/default.toml"
    echo "   IMPORTANT: Edit config before starting service!"
else
    echo "   Config exists, preserved."
fi

# Set ownership
chown -R "$SERVICE_USER:$SERVICE_USER" "$INSTALL_DIR"

# Install systemd service
echo ">> Installing systemd service..."
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SERVICE_FILE=""
for CANDIDATE in \
    "$SCRIPT_DIR/oneclaw.service" \
    "oneclaw.service" \
    "deploy/oneclaw.service"; do
    if [ -f "$CANDIDATE" ]; then
        SERVICE_FILE="$CANDIDATE"
        break
    fi
done

if [ -n "$SERVICE_FILE" ]; then
    cp "$SERVICE_FILE" /etc/systemd/system/oneclaw.service
    systemctl daemon-reload
    echo "   Service installed"
else
    echo "   WARN: oneclaw.service not found, skipping systemd setup"
fi

echo ""
echo "======================================================="
echo "  OneClaw installed to ${INSTALL_DIR}"
echo "======================================================="
echo ""
echo "  Next steps:"
echo "    1. Edit config:  sudo nano ${INSTALL_DIR}/config/default.toml"
echo "    2. Start:        sudo systemctl start oneclaw"
echo "    3. Enable boot:  sudo systemctl enable oneclaw"
echo "    4. Check logs:   journalctl -u oneclaw -f"
echo ""
