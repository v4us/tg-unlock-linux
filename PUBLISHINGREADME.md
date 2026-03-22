# TG Unblock Linux CLI

**Version**: 0.5.0  
**Date**: March 22, 2026

## Overview

TG Unblock Linux CLI is a command-line tool to bypass Telegram blocking via WebSocket tunnel through `web.telegram.org`. This is a minimal, headless version of the original [tg-unblock project](https://github.com/by-sonic/tglock), designed for Linux systems without GUI dependencies.

## Original Project

This project is a derivative of the original [tg-unlock](https://github.com/by-sonic/tglock) by **by sonic**.

Original project features:
- Windows desktop GUI application
- SOCKS5 proxy with WebSocket tunnel
- Automatic DNS configuration
- GoodbyeDPI integration
- Full Telegram DC mapping

## What Changed in This Linux CLI Version

### Removed (Windows/GUI-specific):
- GUI elements (`eframe`, `egui`)
- Windows-specific code (`winapi`, `open` crate)
- Batch file for Windows
- GoodbyeDPI integration (`bypass.rs`)
- Network diagnostic tools (`network.rs`)
- UTF-8 font embedding for Windows

### Added (Authentication - Security Enhancement):
- SOCKS5 username/password authentication (RFC 1929)
- Configurable via environment variables
- Backward compatible (no auth by default)
- **Constant-time comparison** using `subtle` crate
- **No username logging** on auth failures

### Added (Daemon Support):
- Systemd service file
- Installation/uninstallation scripts
- Documentation for production deployment

## Features

- **Lightweight**: Single ~3.7MB binary, no dependencies
- **Secure**: RFC 1929 authentication with constant-time comparison
- **Efficient**: Direct WebSocket tunnel, no intermediate servers
- **Reliable**: No reconnections required, full speed
- **Linux Ready**: Systemd service for persistent operation
- **Well Documented**: Comprehensive DC mapping per official mtproto spec

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
git clone https://github.com/your-username/tglock.git
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

## Telegram Data Center Mapping

Based on [official Telegram MTProto documentation](https://core.telegram.org/mtproto/transports)

| DC | IP Range (149.154.x.x) | IP Range (91.108.x.x) | WebSocket Endpoint |
|----|----------------------|----------------------|-------------------|
| 1 | 149.154.160.0/24 to 149.154.163.0/24 | - | kws1.web.telegram.org |
| 2 | 149.154.164.0/24 to 149.154.167.0/24 | 91.105.x.0/24, 185.76.x.0/24 | kws2.web.telegram.org |
| 3 | 149.154.168.0/24 to 149.154.171.0/24 | 91.108.8.0/22, 91.108.12.0/24 | kws3.web.telegram.org |
| 4 | - | 91.108.12.0/24 (12-15) | kws4.web.telegram.org |
| 5 | - | 91.108.56.0/22 (56-59) | kws5.web.telegram.org |

## Security Features

- **Constant-time comparison** - prevents timing attacks on credentials
- **No credential logging** - prevents user enumeration
- **Direct IP packet path** - no intermediate servers
- **End-to-end encryption** - MTProto remains encrypted through WebSocket
- **Local-only binding** - SOCKS5 proxy only accessible from localhost

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

Based on the [original MIT license](LICENSE) from [by-sonic/tglock](https://github.com/by-sonic/tglock).

This derivative work is licensed under the same MIT license as the original project.

Copyright (c) 2026 by sonic (original project)  
Copyright (c) 2026 Tony Walker (Linux CLI derivative)

Trading the GUI for a CLI, removing Windows-specific dependencies, and adding authentication and daemon support. All core functionality preserved.

## Author

**Original**: by sonic ([@bysonicvpn_bot](https://t.me/bysonicvpn_bot))  
**Linux CLI version with auth + daemon**: Modified by Tony Walker for Linux without GUI dependencies.

## Acknowledgments

This project is inspired by and based on the original [tg-unlock project](https://github.com/by-sonic/tglock) by by sonic. The original project provided the foundation for this Linux CLI adaptation.

## Security Review

This version has been reviewed by:
- Qwen/Qwen3-Coder-480B-A35B-Instruct
- Security fixes verified and implemented
- Constant-time comparison using subtle crate
- No credential logging on auth failures

---

*This is a Linux CLI derivative of tg-unlock*  
*All core functionality preserved, GUI removed, auth and daemon support added*  
*Telegram DC mapping documented per official mtproto specification*  
*Security fixes implemented per code review findings*
