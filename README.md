# TG Unblock - Linux CLI Version

**Version**: 0.5.0  
**Date**: March 23, 2026

## Overview

TG Unblock CLI is a command-line tool to bypass Telegram blocking via WebSocket tunnel through `web.telegram.org`. This is a minimal, headless version of the original tg-unblock project, designed for Linux systems without GUI dependencies.

## Security & Performance Fixes (Version 0.5.0 - March 2026)

### Memory Leak Fixes
- Added **FIFO connection tracker** per IP with 100 concurrent connections limit (prevents memory pileup under flood attacks)
- WebSocket connection timeout: **10 seconds** (prevents hung connections)
- Ping interval: **5 seconds** (maintains stable connections)
- Proper connection cleanup on tunnel closure

### Flood Attack Protection
- Limits concurrent connections to **100 per IP** (raised from 10)
- Over-limit connections trigger FIFO eviction (oldest first)
- Prevents memory exhaustion from botnet-style attacks
- Connection rate limiting built into tracker logic

### Error Handling
- WebSocket connection timeout detection and cleanup
- Proper error propagation for failed tunnels
- Connection slots released after tunnel end

## Security Features

- **Constant-time comparison** - prevents timing attacks on credentials
- **No username logging** - prevents user enumeration
- **Direct IP packet path** - no intermediate servers
- **End-to-end encryption** - MTProto remains encrypted through WebSocket
- **Local-only binding** - SOCKS5 proxy only accessible from localhost

## What Changed

### Removed (Windows/GUI-specific):
- GUI elements (`eframe`, `egui`)
- Windows-specific code (`winapi`, `open` crate)
- Batch file for Windows
- GoodbyeDPI integration (`bypass.rs`)
- Network diagnostic tools (`network.rs`)
- UTF-8 font embedding for Windows

### Added (Authentication):
- SOCKS5 username/password authentication (RFC 1929)
- Configurable via environment variables
- Backward compatible (no auth by default)
- Secure credential handling
- Constant-time comparison to prevent timing attacks
- No username logging on auth failures (prevents enumeration)

### Added (DC Documentation):
- Full Telegram DC mapping documentation
- Official DC ranges from mtproto spec
- WebSocket endpoint naming (kws1-5.web.telegram.org)
- Comments explaining DC extraction

### Kept (Core functionality):
- SOCKS5 proxy implementation
- WebSocket tunnel to Telegram DCs
- DC extraction from obfuscated2 packets
- IP-based DC mapping
- AES-256-CTR decryption for obfuscated2
- Bi-directional relay between TCP and WebSocket
- PING/PONG handling

## File Structure

```
tglock/
├── Cargo.toml              # Dependencies (clap, log, env_logger, subtle)
├── README.md               # Project documentation
├── README_LINUX.md         # Linux CLI documentation
├── DAEMON_SETUP.md         # Systemd daemon configuration
├── HABR_ARTICLE.md         # Original technical explanation (Russian)
├── tg_blacklist.txt        # Telegram IPs/domains
├── src/
│   ├── lib.rs              # Library exports
│   ├── cli.rs              # CLI entry point
│   └── ws_proxy.rs         # Core SOCKS5 + WebSocket logic
├── tg_unblock.service      # Systemd service file
├── install_daemon.sh       # Daemon installation script
└── uninstall_daemon.sh     # Daemon removal script
```

## Dependencies

- **tokio** - Async runtime
- **tokio-tungstenite** - WebSocket client with TLS
- **native-tls** - System TLS (no OpenSSL needed on most systems)
- **futures-util** - Stream/sink utilities
- **aes**, **ctr**, **cipher** - DC extraction decryption
- **clap** - CLI argument parsing
- **log**, **env_logger** - Logging
- **subtle** - Constant-time comparison for security

## Usage

```bash
# Basic usage (port 1080, no auth)
./tg_unblock

# With custom port
./tg_unblock --port 1081

# Verbose logging
./tg_unblock -v

# Show version
./tg_unblock --version

# Show help
./tg_unblock --help
```

### With Authentication

```bash
export TG_UNBLOCK_AUTH=1
export TG_UNBLOCK_USERNAME=myuser
export TG_UNBLOCK_PASSWORD=mypassword
./tg_unblock -v
```

## Building from Source

```bash
# Install dependencies (Ubuntu/Debian)
sudo apt-get install libssl-dev pkg-config

# Clone and build
git clone https://github.com/by-sonic/tglock.git
cd tglock
cargo build --release

# Binary will be at: target/release/tg_unblock
```

## System Requirements

- **OS**: Linux (any distribution)
- **Rust**: 1.70+
- **Network**: HTTPS outbound to web.telegram.org
- **Permissions**: Standard user (no root needed for basic use)

## How It Works

```
Telegram Desktop (client)
       │
       ▼ SOCKS5
  127.0.0.1:1080
       │
       ▼
  tg_unblock (CLI)
       │
       ├── Telegram DC? ──► WSS → web.telegram.org
       │                    (looks like HTTPS)
       │
       └── Other IP? ─────► Direct TCP (passthrough)
```

## Command-line Options

```
Telegram Unblock - Command-line WebSocket proxy for bypassing Telegram blocking

Usage: tg_unblock [OPTIONS]

Options:
  -b, --bind <BIND>     Bind address for SOCKS5 proxy [default: 127.0.0.1]
  -p, --port <PORT>     Port to listen on for SOCKS5 connections [default: 1080]
  -v, --verbose         Enable verbose logging (debug level)
      --version         Show version and exit
  -h, --help            Print help
```

**Server Deployment Example:**
```bash
# Localhost only (recommended for security)
./tg_unblock --bind 127.0.0.1 --port 8888 -v

# Remote access (use firewall for protection)
./tg_unblock --bind 0.0.0.0 --port 8888 -v
```

## Testing

### 1. Without authentication (default)
```bash
./tg_unblock -v
```

### 2. With authentication (optional)
```bash
export TG_UNBLOCK_AUTH=1
export TG_UNBLOCK_USERNAME=myuser
export TG_UNBLOCK_PASSWORD=mypassword

./tg_unblock -v
```

### 3. Configure Telegram Desktop
- Settings → Advanced → Connection type → **Use SOCKS5 proxy**
- Server: `127.0.0.1`, Port: `1080`
- If auth is enabled, enter your credentials
- If auth is disabled, leave login/password empty

### 4. Connect
Click "Connect" in Telegram, it should work immediately.

## Usage with Authentication

When authentication is enabled, clients must provide valid credentials:

```
# Client sends:
0x05 0x01 0x02     # SOCKS5, 1 method, auth with user/pass

# Server offers:
0x05 0x01 0x02     # SOCKS5, 1 method, auth with user/pass

# Client sends auth:
0x01 0x05 user 0x05 pass  # Version, username length=5, password length=5

# Server responds:
0x01 0x00   # Success!
```

### Trusted IP Auto-Auth Bypass

To improve user experience with multiple connected clients, the daemon supports automatic auth bypass for trusted IPs:

**How it works:**
1. When a client successfully authenticates, their IP is recorded as "trusted"
2. If the same IP connects again within 10 minutes, auth is automatically bypassed
3. After 10 minutes of inactivity, the IP is removed from trusted list
4. Expired entries are cleaned up every 5 minutes to prevent memory leaks

**Benefits:**
- Eliminates need to re-enter credentials for frequently connecting clients
- Maintains security by expiring trusted status after 10 minutes
- Works automatically - no configuration needed
- Prevents memory leaks with automatic cleanup

**Use cases:**
- Multiple devices from the same network
- Mobile clients that reconnect after backgrounding
- Apps that open multiple connections

**Security considerations:**
- Only trusted locally (within daemon memory)
- IPs expire after 10 minutes of inactivity
- No permanent storage of IP records
- Clients must still provide valid credentials for first connection

### Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `TG_UNBLOCK_AUTH` | Enable auth (`1`/`true`) | `export TG_UNBLOCK_AUTH=1` |
| `TG_UNBLOCK_USERNAME` | Auth username | `export TG_UNBLOCK_USERNAME=myuser` |
| `TG_UNBLOCK_PASSWORD` | Auth password | `export TG_UNBLOCK_PASSWORD=mypassword` |
| `TG_UNBLOCK_TRUSTED_EXPIRY` | Trusted IP expiry in seconds | `export TG_UNBLOCK_TRUSTED_EXPIRY=600` (default 600) |

### Notes
- **No auth by default** - for backward compatibility with existing clients
- **Secure by default** - credentials are read from environment (not CLI args or config files)
- **Client must support SOCKS5 auth (RFC 1929)** - most modern clients do (Telegram Desktop, browsers, etc.)
- **Timing attack prevention** - uses constant-time comparison
- **No username logging** - prevents user enumeration attacks
- **Trusted IP auto-auth** - successful connections are tracked; IPs within 10-min window get auto-auth bypass (configurable via `TG_UNBLOCK_TRUSTED_EXPIRY`)

## Telegram Data Center Mapping

Based on [official Telegram MTProto documentation](https://core.telegram.org/mtproto/transports)

| DC | IP Range (149.154.x.x) | IP Range (91.108.x.x) | WebSocket Endpoint |
|----|----------------------|----------------------|-------------------|
| 1 | 149.154.160.0/24 to 149.154.163.0/24 | - | wss://kws1.web.telegram.org/apiws |
| 2 | 149.154.164.0/24 to 149.154.167.0/24 | 91.105.x.0/24, 185.76.x.0/24 | wss://kws2.web.telegram.org/apiws |
| 3 | 149.154.168.0/24 to 149.154.171.0/24 | 91.108.8.0/22, 91.108.12.0/24 | wss://kws3.web.telegram.org/apiws |
| 4 | - | 91.108.12.0/24 (12-15) | wss://kws4.web.telegram.org/apiws |
| 5 | - | 91.108.56.0/22 (56-59) | wss://kws5.web.telegram.org/apiws |

### DC Extraction from Obfuscated2

The first 64 bytes of an MTProto obfuscated2 connection contain:
- Bytes 8-39: AES key (32 bytes)
- Bytes 40-55: AES IV (16 bytes)
- Bytes 60-63: Encrypted DC ID (little-endian)

The DC ID is extracted by:
1. Decrypting with AES-256-CTR using the contained key/IV
2. Reading the last 4 bytes as little-endian
3. Taking absolute value and validating (1-5)

## Daemon Setup

### Quick Install
```bash
sudo ./install_daemon.sh
```

### Manual Setup
1. Copy `tg_unblock.service` to `/etc/systemd/system/`
2. Edit to set your authentication credentials
3. Run: `sudo systemctl daemon-reload && sudo systemctl enable tg_unblock`

See `DAEMON_SETUP.md` for full documentation.

## Security Features

- **Constant-time comparison** - prevents timing attacks on credentials
- **No username logging** - prevents user enumeration
- **Direct IP packet path** - no intermediate servers
- **End-to-end encryption** - MTProto remains encrypted through WebSocket
- **Local-only binding** - SOCKS5 proxy only accessible from localhost

## Security Recommendations (from code review)

Based on review by Qwen/Qwen3-Coder-480B-A35B-Instruct:

1. ✅ **Timing attack fixed** - Using byte-by-byte comparison
2. ✅ **No credential logging** - Username removed from auth failure logs
3. ✅ **Connection rate limiting** - Added 100-connection limit per IP with FIFO eviction
4. ✅ **WebSocket timeout** - 10-second timeout prevents connection pileup
5. ⚠️ **Certificate pinning** - Consider implementing for WebSocket connections

## Credits

- Original tg-unlock by by sonic ([@bysonicvpn_bot](https://t.me/bysonicvpn_bot))
- Linux CLI adaptation by Tony Walker
- Memory leak fixes and flood protection by Qwen/Qwen3-Coder models

## Comparison with Original

| Feature | Original (Windows GUI) | CLI Version |
|---------|----------------------|-------------|
| OS | Windows only | Linux only |
| GUI | eframe/egui | None (console) |
| Port | 1080 (fixed) | Configurable via `--port` |
| Logging | GUI log panel | stdout (with --verbose) |
| DNS setting | Auto (admin) | N/A |
| GoodbyeDPI fallback | Yes | No |
| Size | ~6MB | ~3.7MB (optimized) |
| Auth | N/A | Yes (RFC 1929, constant-time) |
| DC docs |None| Complete from mtproto spec |

## License

MIT - Same as original project.

Copyright (c) 2026 Tony Walker (Linux CLI version)  
Copyright (c) 2026 by sonic (original project)

## Author

**Linux CLI Version**: Tony Walker  
A Linux CLI adaptation of tg-unlock with authentication, systemd daemon support, and comprehensive DC documentation.

**Based on**: Original tg-unlock by by sonic ([@bysonicvpn_bot](https://t.me/bysonicvpn_bot))

**Collaboration**: v4us (repository owner and maintainer)

---

_This is a Linux CLI variant of tg-unlock_  
_All core functionality preserved, GUI removed, auth and daemon support added_  
_Telegram DC mapping documented per official mtproto specification_  
_Security fixes implemented with code review by Qwen/Claude models_

---

## Acknowledgments

Based on the original [tg-unlock](https://github.com/by-sonic/tglock) project by by sonic.
