<p align="center">
  <img src="assets/icon.svg" width="120" height="120" alt="Conan Exiles Server Manager" />
</p>

<h1 align="center">Conan Exiles: Enhanced Server Manager</h1>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-backend-b7410e?style=flat-square&logo=rust&logoColor=white" alt="Rust backend" />
  <img src="https://img.shields.io/badge/TypeScript-frontend-3178c6?style=flat-square&logo=typescript&logoColor=white" alt="TypeScript frontend" />
  <img src="https://img.shields.io/badge/Tauri-2.x-24c8db?style=flat-square&logo=tauri&logoColor=white" alt="Tauri 2" />
  <img src="https://img.shields.io/badge/License-MIT-yellow?style=flat-square" alt="MIT License" />
  <img src="https://img.shields.io/badge/Windows-10%20%7C%2011-blue?style=flat-square&logo=windows&logoColor=white" alt="Windows" />
</p>

<p align="center">
  A sleek, low-overhead desktop app for hosting <strong>Conan Exiles: Enhanced</strong> (UE5) dedicated servers.<br/>
  Install, configure every setting, manage mods and players, back up, schedule and repair – all from one window.
</p>

---

## ✨ What it does

| | |
|---|---|
| 🏗️ **One-click server install** | Downloads the Conan Exiles Enhanced dedicated server via SteamCMD (AppID `443030`, branch `public`), sets up the folder, and gets you to a running server in minutes. |
| 🎛️ **Every setting, plain language** | Max players, tick rate, difficulty, loot, XP multipliers, platform policies, crash dumps – all editable in a clean UI. Changes apply live or on next restart. |
| 🧩 **Workshop mod management** | Browse, install, enable/disable and update Steam Workshop mods per server. Shared download cache means one copy of each mod for all your servers. |
| 📡 **Live telemetry & RCON** | Real-time player count, server state, log tail, and a full RCON console – no SSH, no third-party tools. |
| 💾 **Backups & automation** | Scheduled backups, backup-on-stop, auto-restart on crash, freeze detection, timed restarts, and webhook notifications (Discord/Slack). |
| 🖥️ **Multi-server** | Run and manage as many servers as you want from one window. Clone a server to spin up a new one with the same config. |
| 📦 **Portable or installed** | Use the installer (data in `%APPDATA%`) or the portable zip (everything stays in the folder – move it to another drive and it just works). |

---

## 🚀 Quick start

### Option A – Installer (recommended)

1. Download **`ConanExiles-Server-Manager-Setup.exe`** from [**Releases**](https://github.com/clawdroppin/ConanExilesEnhanced-ServerManager/releases/latest) and run it.
2. Launch **Conan Exiles Enhanced Server Manager** from the Start menu.
3. Click **Install Server**, pick a folder, and wait for SteamCMD to finish.
4. Configure your settings, hit **Start**, and you're live.

### Option B – Portable

1. Download **`ConanExiles-Server-Manager-Portable.zip`** from [**Releases**](https://github.com/clawdroppin/ConanExilesEnhanced-ServerManager/releases/latest).
2. Unzip anywhere writable (e.g. `D:\Games\ConanExiles-ServerManager`).
3. If Windows marks the zip as "from the internet", right-click → **Properties** → tick **Unblock** → OK *before* unzipping.
4. Double-click **`Conan Exiles Server Manager.exe`**.

> The `portable.txt` file next to the exe keeps all data (settings, servers, SteamCMD, backups) inside the folder. Delete it to switch to `%APPDATA%` mode.

---

## 📋 Requirements

- **Windows 10 or 11** (64-bit)
- **Microsoft Edge WebView2 runtime** – ships with Windows 10/11 by default
- **~15 GB free disk** for a server install (game files + saves)
- **SteamCMD** – downloaded automatically on first install, no Steam account needed
- **Outbound network** to `steamcontent.com` for server files and Workshop mods

---

## 🧭 Features in detail

### Server lifecycle
- **Install** – SteamCMD `app_update 443030` with live progress bar
- **Start / Stop / Restart** – graceful RCON shutdown → Ctrl+Break → force-kill (configurable timeout)
- **Update** – stops the server, pulls the latest build, restarts. Optional "update & restart" one-click
- **Delete** – removes the server profile; optionally deletes the install folder (backups are preserved)
- **Clone** – duplicate a server's config and (optionally) its saves into a new profile

### Configuration
- All `DedicatedServer.ini` fields exposed in a structured UI
- Game rules (`ServerSettings` section) editable with plain-language labels
- Changes saved while the server is running are applied automatically on next stop/restart
- INI round-trips losslessly – unknown keys, comments, and ordering are preserved

### Mods & Workshop
- Per-server mod list with enable/disable toggles
- Shared Workshop download cache (one copy per mod, referenced by all servers)
- Mod sync on server start (ensures the right mods are in the right folder)
- Optional Steam Web API key for in-app Workshop search

### Monitoring & automation
- Live player count, server state, uptime, log tail
- RCON console (TCP) with command history
- **Supervisor** – watches for crashes and frozen servers, auto-restarts
- **Freeze detection** – no log output + no RCON response for N minutes → restart
- **Scheduled restarts** – daily at a set time
- **Backup on stop** – automatic snapshot before every shutdown
- **Webhook notifications** – Discord/Slack-compatible, configurable event levels
- **Streamer mode** – masks IPs, passwords, and player IDs in the UI

### Data & security
- RCON passwords and webhook URLs encrypted with Windows DPAPI
- Atomic JSON writes with automatic backup of the previous state
- All app data in one folder (portable) or `%APPDATA%` (installed)
- Daily rolling log files in `<data>/logs/`

---

## 🧑‍💻 Build from source

```shell
git clone https://github.com/clawdroppin/ConanExilesEnhanced-ServerManager.git
cd ConanExilesEnhanced-ServerManager
npm install

# Run in dev mode (hot reload)
npm run tauri dev

# Build everything (installer + portable)
npm run build:all
```

**Prerequisites:**
- [Rust](https://rustup.rs/) (stable toolchain)
- [Node.js](https://nodejs.org/) 20+
- [Windows SDK](https://developer.microsoft.com/windows/downloads/windows-sdk/) (for Tauri)
- WebView2 runtime (pre-installed on Win 10/11)

---

## 📁 Project structure

```
├── src/                    # React 19 + TypeScript frontend
│   ├── App.tsx             # Root component
│   ├── components/         # Reusable UI (charts, layout, primitives)
│   ├── features/           # Feature modules (dashboard, dialogs, server)
│   ├── hooks/              # React hooks (backend events, etc.)
│   ├── lib/                # IPC bridge, types, dev mock
│   ├── store/              # Zustand state
│   └── data/               # Static data (settings definitions)
├── src-tauri/              # Rust backend (Tauri 2)
│   └── src/
│       ├── lib.rs          # App entry, tray, window management
│       ├── commands.rs     # All #[tauri::command] handlers
│       ├── models.rs       # Shared types (serde, camelCase)
│       ├── state.rs        # AppState, persistence, runtime
│       ├── paths.rs        # Data directory resolution (portable vs installed)
│       └── core/           # Business logic
│           ├── server.rs   # Start/stop/restart/update lifecycle
│           ├── steamcmd.rs # SteamCMD execution + progress parsing
│           ├── rcon.rs     # RCON TCP client
│           ├── mods.rs     # Workshop mod management
│           ├── modsync.rs  # Mod folder sync on start
│           ├── backup.rs   # Server backup/restore
│           ├── supervisor.rs # Crash/freeze detection + auto-restart
│           ├── proc.rs     # Process management (kill, scan, Ctrl+Break)
│           ├── ini.rs      # Lossless INI read/write
│           ├── secrets.rs  # DPAPI encryption
│           ├── network.rs  # Port checks, UPnP
│           ├── history.rs  # Audit log, session tracking
│           ├── settings.rs # App settings persistence
│           ├── gamedb.rs   # Game database queries
│           ├── diagnostics.rs # Health checks
│           ├── download.rs # File download helper
│           └── upnp.rs     # UPnP port forwarding
├── scripts/
│   ├── build-portable.mjs  # Portable packaging script
│   └── screenshots.mjs     # Screenshot capture (dev)
├── .github/workflows/
│   ├── ci.yml              # Typecheck + lint + test on every push
│   └── release.yml         # Tag-triggered release build
├── assets/                 # App icon
└── docs/                   # Design notes
```

---

## 🤖 For AI agents and contributors

- 📘 **[AGENTS.md](AGENTS.md)** – the complete guide: architecture, data flow, conventions, Steam AppIDs, build/test commands, and gotchas. **Read this before making any changes.**
- 🤝 **[CLAUDE.md](CLAUDE.md)** – quick-reference pointer for Claude Code sessions.
- 📝 **[CHANGELOG.md](CHANGELOG.md)** – what changed in each version.

---

## 📄 License

[MIT](LICENSE) © [clawdroppin](https://github.com/clawdroppin)

---

<p align="center"><sub>Not affiliated with Funcom or Amazon Games. Conan Exiles is a trademark of its respective owner.</sub></p>
