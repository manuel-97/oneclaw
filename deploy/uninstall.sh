#!/bin/bash
# OneClaw Uninstall Script
# Removes service, binary, config, user, and all data.
#
# Usage: sudo ./uninstall.sh
#
# WARNING: This removes /opt/oneclaw including all data!

set -e

# Must run as root
if [ "$EUID" -ne 0 ]; then
    echo "  ERROR: Please run with sudo"
    exit 1
fi

echo "======================================================="
echo "  OneClaw Uninstaller"
echo "======================================================="
echo ""

# Stop and disable service
echo ">> Stopping service..."
systemctl stop oneclaw 2>/dev/null || true
systemctl disable oneclaw 2>/dev/null || true

echo ">> Removing systemd unit..."
rm -f /etc/systemd/system/oneclaw.service
systemctl daemon-reload

echo ">> Removing installation directory..."
rm -rf /opt/oneclaw

echo ">> Removing service user..."
userdel oneclaw 2>/dev/null || true

echo ""
echo "  OneClaw uninstalled."
echo ""
