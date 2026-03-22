# TG Unblock - Linux CLI Version

**Version**: 0.4.0  
**Date**: March 22, 2026

## Overview

TG Unblock CLI is a command-line tool to bypass Telegram blocking via WebSocket tunnel through `web.telegram.org`. This is a minimal, headless version of the original tg-unblock project, designed for Linux systems without GUI dependencies.

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
├── Cargo.toml              # Dependencies (clap, log, env_logger)
├── README.md               # Original documentation
├── HABR_ARTICLE.md         # Technical explanation (Russian)
├── tg_blacklist.txt        # Telegram IPs/domains
├── src/
│   ├── lib.rs              # Library exports
│   ├── cli.rs              # CLI entry point
│   └── ws_proxy.rs         # Core SOCKS5 + WebSocket logic
└── LICENSE                 # MIT
```

## Dependencies

- **tokio** - Async runtime
- **tokio-tungstenite** - WebSocket client
- **native-tls** - System TLS (no OpenSSL needed on most systems)
- **futures-util** - Stream/sink utilities
- **aes**, **ctr**, **cipher** - DC extraction decryption
- **clap** - CLI argument parsing
- **log**, **env_logger** - Logging (no runtime dep on envfilter)

## Usage

```bash
# Basic usage (port 1080)
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
- **Permissions**: Standard user (no root needed)

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

### Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `TG_UNBLOCK_AUTH` | Enable auth (`1`/`true`) | `export TG_UNBLOCK_AUTH=1` |
| `TG_UNBLOCK_USERNAME` | Auth username | `export TG_UNBLOCK_USERNAME=myuser` |
| `TG_UNBLOCK_PASSWORD` | Auth password | `export TG_UNBLOCK_PASSWORD=mypassword` |

### Notes
- **No auth by default** - for backward compatibility with existing clients
- **Secure by default** - credentials are read from environment (not CLI args or config files)
- **Client must support SOCKS5 auth (RFC 1929)** - most modern clients do (Telegram Desktop, browsers, etc.)

## Comparison with Original

| Feature | Original (Windows GUI) | CLI Version |
|---------|----------------------|-------------|
| OS | Windows only | Linux only |
| GUI | eframe/egui | None (console) |
| Port | 1080 (fixed) | Configurable via `--port` |
| Logging | GUI log panel | stdout (with --verbose) |
| DNS setting | Auto (admin) | N/A |
| GoodbyeDPI fallback | Yes | No |
| Size | ~6MB | ~4MB (optimized) |

## License

MIT - Same as original project.

## Author

Original: by sonic (@bysonicvpn_bot)  
CLI version: Modified for Linux without GUI dependencies.

---

*This is a simplified, Linux-only derivative of tg-unblock*  
*All core functionality preserved, GUI removed*
