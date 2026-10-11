# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.1] – 2026-10-11

### Added
- **Map selection** when creating a server and in Settings – pick Exiled Lands, Isle of Siptah, or any other map
- **Automatic map discovery**: the manager scans the installed game for `.umap` assets, so new official maps and mod maps appear in the dropdown with no code change
- **Console command input** in the Console tab – type a command and press Enter to run it against the live server (RCON), with up/down arrow history
- **Max players**: slider *and* a typed number field (Funcom supports up to 100)

### Fixed
- In-app title-bar logo now matches the app file icon (shield + C + crossed swords)
- RCON response lines in the Console are labeled `rcon` (previously mislabeled `steam`)

## [1.0.0] – 2026-10-11

### Added
- Full server lifecycle: install, start, stop, restart, update, delete, clone
- Structured UI for all `DedicatedServer.ini` settings and game rules
- Steam Workshop mod management with shared download cache
- Live telemetry: player count, server state, log tail, RCON console
- Backup system: manual, scheduled, backup-on-stop
- Automation: auto-restart on crash, freeze detection, scheduled restarts
- Webhook notifications (Discord/Slack-compatible)
- Multi-server management from a single window
- Portable mode (data stays in the app folder) and installed mode (`%APPDATA%`)
- Streamer mode (masks IPs, passwords, player IDs)
- DPAPI encryption for sensitive values (RCON passwords, webhook URLs)
- UPnP port forwarding helper
- Tray icon with close-to-tray option
- Desktop notifications
- Daily rolling log files

### Fixed
- Delete server now force-kills processes directly (no unnecessary backup or config write)
- All "Dragonwilds" references removed from codebase
