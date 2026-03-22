#!/bin/bash
# tg_unblock daemon removal script
# Usage: sudo ./uninstall_daemon.sh

set -e

echo "=== TG Unblock Daemon Removal ==="

# Check if running as root
if [ "$EUID" -ne 0 ]; then 
    echo "Please run as root (sudo $0)"
    exit 1
fi

SERVICE_FILE="/etc/systemd/system/tg_unblock.service"

# Check if service exists
if [ ! -f "$SERVICE_FILE" ]; then
    echo "Service not installed at $SERVICE_FILE"
    exit 0
fi

echo "Stopping service..."
systemctl stop tg_unblock.service 2>/dev/null || true

echo "Disabling service..."
systemctl disable tg_unblock.service

echo "Removing service file..."
rm -f "$SERVICE_FILE"

echo "Reloading systemd daemon..."
systemctl daemon-reload

echo ""
echo "=== Removal Complete ==="
echo "tg_unblock service has been removed from the system."
