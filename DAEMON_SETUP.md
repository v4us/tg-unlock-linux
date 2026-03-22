# TG Unblock - Daemon Setup Guide

**Version**: 0.5.0  
**Date**: March 22, 2026

## Systemd Service Configuration

TG Unblock is ready to run as a systemd service for persistent daemon operation.

### Quick Start (Simple)

```bash
# Make script executable
chmod +x /home/user1/.openclaw/workspace/tglock/*.sh

# Run installation (requires root)
sudo /home/user1/.openclaw/workspace/tglock/install_daemon.sh
```

### Manual Setup

#### 1. Create service file

```bash
sudo tee /etc/systemd/system/tg_unblock.service << 'EOF'
[Unit]
Description=Telegram Unblock - SOCKS5 WebSocket Proxy
After=network.target

[Service]
Type=simple
User=user1
Group=users
WorkingDirectory=/home/user1/.openclaw/workspace/tglock
ExecStart=/home/user1/.openclaw/workspace/tglock/target/release/tg_unblock
StandardOutput=journal
StandardError=journal
SyslogIdentifier=tg_unblock
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF
```

#### 2. Configure Environment Variables

Edit the service file to set authentication credentials and bind address:

```bash
sudo nano /etc/systemd/system/tg_unblock.service
```

Add these lines in the `[Service]` section:

```ini
Environment="TG_UNBLOCK_AUTH=1"
Environment="TG_UNBLOCK_USERNAME=your_username"
Environment="TG_UNBLOCK_PASSWORD=your_secure_password"
Environment="RUST_LOG=info"

# Bind address options:
# - 127.0.0.1 (default, localhost only, most secure)
# - 0.0.0.0 (all interfaces, remote access)
ExecStart=/home/user1/.openclaw/workspace/tglock/target/release/tg_unblock --bind 127.0.0.1
```

#### 3. Reload and Enable

```bash
# Reload systemd
sudo systemctl daemon-reload

# Enable on boot
sudo systemctl enable tg_unblock

# Start immediately
sudo systemctl start tg_unblock
```

### Usage

```bash
# Start service
sudo systemctl start tg_unblock

# Stop service
sudo systemctl stop tg_unblock

# Restart service
sudo systemctl restart tg_unblock

# Check status
sudo systemctl status tg_unblock

# View logs
sudo journalctl -u tg_unblock -f

# View recent logs
sudo journalctl -u tg_unblock --since "1 hour ago"
```

### Configuration Options

#### Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `TG_UNBLOCK_AUTH` | Enable auth (1/true) | `1` |
| `TG_UNBLOCK_USERNAME` | Auth username | `tgproxy` |
| `TG_UNBLOCK_PASSWORD` | Auth password | `secure_password` |
| `RUST_LOG` | Log level | `info`, `debug` |

#### Port Configuration

To change the listening port, modify the service file:

```ini
ExecStart=/home/user1/.openclaw/workspace/tglock/target/release/tg_unblock -p 1081
```

Or use environment variable:

```ini
Environment="TG_PORT=1081"
```

Then in `cli.rs`, handle the env variable:

```rust
let port = env::var("TG_PORT")
    .ok()
    .and_then(|p| p.parse().ok())
    .unwrap_or(1080);
```

### Security Best Practices

1. **Use strong passwords**: Generate secure passwords with `openssl rand -base64 32`

2. **Run as non-root**: Service already runs as `user1` (not root)

3. **Use environment files**: Store credentials in separate file with restricted permissions:

```bash
sudo tee /etc/tg_unblock.env << 'EOF'
TG_UNBLOCK_AUTH=1
TG_UNBLOCK_USERNAME=tgproxy
TG_UNBLOCK_PASSWORD=$(openssl rand -base64 32)
EOF

sudo chmod 600 /etc/tg_unblock.env
```

Then reference in service file:

```ini
EnvironmentFile=/etc/tg_unblock.env
```

4. **Restrict to localhost**: Default binds to 127.0.0.1 only

5. **Enable firewall**: Allow only necessary connections

```bash
sudo ufw allow from 127.0.0.1 to any port 1080
```

### Monitoring

```bash
# Check if running
systemctl is-active tg_unblock

# View last 100 log lines
journalctl -u tg_unblock -n 100 --no-pager

# Follow live logs
journalctl -u tg_unblock -f
```

### Troubleshooting

#### Service won't start

```bash
# Check status
sudo systemctl status tg_unblock

# Check logs
sudo journalctl -u tg_unblock -n 50 --no-pager
```

#### Port already in use

```bash
# Check what's using port 1080
sudo lsof -i :1080
sudo netstat -tlnp | grep :1080
```

#### Authentication failing

```bash
# Check logs for auth errors
sudo journalctl -u tg_unblock | grep -i auth

# Verify environment variables
sudo systemctl show tg_unblock | grep Environment
```

### Removal

```bash
sudo /home/user1/.openclaw/workspace/tglock/uninstall_daemon.sh
```

Or manual:

```bash
sudo systemctl stop tg_unblock
sudo systemctl disable tg_unblock
sudo rm /etc/systemd/system/tg_unblock.service
sudo systemctl daemon-reload
```

### systemd Features Available

The service uses these systemd features for robustness:

- **Restart=on-failure**: Automatically restarts on crashes
- **RestartSec=10**: Wait 10s before restart
- **PrivateTmp=true**: Separate tmp directory
- **NoNewPrivileges=true**: Security hardening
- **ProtectSystem=strict**: Read-only system files
- **ProtectHome=true**: Read-only home directory

---

**Author**: Tony Walker (modified from original tg-unblock by by sonic)  
**Date**: March 22, 2026  
**Version**: 0.4.0 (Linux CLI with auth)
