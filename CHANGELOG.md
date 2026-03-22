# TG Unblock - Changelog

**Version**: 0.5.0  
**Date**: March 22, 2026

## [0.5.0] - 2026-03-22

### Added
- Linux CLI version (headless, no GUI)
- RFC 1929 authentication support
- Constant-time comparison using subtle crate
- Systemd service configuration
- Installation/uninstallation scripts
- Comprehensive DC mapping documentation
- Daemon setup guide

### Changed
- Version bump from 0.4.0 to 0.5.0
- Updated dependencies (added subtle crate)
- Simplified file structure (removed Windows-specific files)

### Security
- Timing attack prevention (constant-time credential comparison)
- No username logging on authentication failures
- All security issues from code review addressed

### Breaking Changes
- Removed Windows GUI dependencies (eframe, egui)
- Removed Windows-specific code (winapi)
- Removed GoodbyeDPI integration
- Removed network diagnostic tools

---


---

## TG Unblock Linux CLI Changelog

This changelog documents the Linux CLI derivative of the original tg-unlock project. The original project is maintained at [https://github.com/by-sonic/tglock](https://github.com/by-sonic/tglock).
