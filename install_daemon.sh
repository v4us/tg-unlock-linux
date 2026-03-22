#!/bin/bash
# tg_unblock daemon setup script
# Usage: sudo ./install_daemon.sh

set -e

echo "=== TG Unblock Daemon Setup ==="

# Check if running as root
if [ "$EUID" -ne 0 ]; then 
    echo "Please run as root (sudo $0)"
    exit 1
fi

# Service file location
SERVICE_FILE="/etc/systemd/system/tg_unblock.service"
BINARY_PATH="/home/user1/.openclaw/workspace/tglock/target/release/tg_unblock"

# Check if binary exists
if [ ! -f "$BINARY_PATH" ]; then
    echo "Error: Binary not found at $BINARY_PATH"
    echo "Please build first: cd /home/user1/.openclaw/workspace/tglock && cargo build --release"
    exit 1
fi

# Check if service file exists
if [ -f "$SERVICE_FILE" ]; then
    echo "Service file already exists at $SERVICE_FILE"
    echo "Skipping creation..."
else
    echo "Creating service file..."
    
    # Create the unit file
    cat > "$SERVICE_FILE" << 'EOF'
[Unit]
Description=Telegram Unblock - SOCKS5 WebSocket Proxy
Documentation=https://github.com/by-sonic/tglock
After=network.target

[Service]
Type=simple
User=user1
Group=users
WorkingDirectory=/home/user1/.openclaw/workspace/tglock
ExecStart=/home/user1/.openclaw/workspace/tglock/target/release/tg_unblock
Environment="TG_UNBLOCK_AUTH=1"
Environment="TG_UNBLOCK_USERNAME=tgproxy"
# NOTE: Set your secure password in the service file or use an env file
# Environment="TG_UNBLOCK_PASSWORD=YOUR_SECURE_PASSWORD_HERE"
StandardOutput=journal
StandardError=journal
SyslogIdentifier=tg_unblock
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF

    echo "Service file created at $SERVICE_FILE"
fi

# Reload systemd
echo "Reloading systemd daemon..."
systemctl daemon-reload

# Enable service
echo "Enabling tg_unblock service..."
systemctl enable tg_unblock.service

# Show status
echo ""
echo "=== Installation Complete ==="
echo ""
echo "To configure authentication, edit: $SERVICE_FILE"
echo "Set your secure username/password in the Environment lines."
echo ""
echo "Common commands:"
echo "  systemctl start tg_unblock     # Start the service"
echo "  systemctl stop tg_unblock      # Stop the service"
echo "  systemctl status tg_unblock    # Check status"
echo "  journalctl -u tg_unblock -f  # View logs"
echo ""
echo "Current status:"
systemctl status tg_unblock --no-pager
