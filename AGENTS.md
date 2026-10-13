# AGENTS.md – Guide for AI assistants and contributors

This file is the hand-off document for anyone (human or AI model) continuing work on **Conan Exiles: Enhanced Server Manager**, a Windows desktop app for running *Conan Exiles: Enhanced* (UE5) dedicated servers. Read it fully before changing code. `CLAUDE.md` points here.

---

## 1. What the app is

A Tauri 2 desktop app (Rust backend + React 19 / TypeScript frontend) that:
- Installs the Conan Exiles Enhanced dedicated server via SteamCMD
- Manages server lifecycle (start, stop, restart, update, delete, clone)
- Exposes every `ServerSettings.ini` game rule (~290 keys, Game Rules view) plus the profile-owned identity/network/RCON settings (Server Settings view)
- Manages bans and the whitelist (`Saved/blacklist.txt`, `Saved/whitelist.txt`)
- Manages Steam Workshop mods per server (with a shared download cache)
- Provides live telemetry, RCON console, log tail
- Automates: backups, scheduled restarts, crash recovery, freeze detection
- Sends webhook notifications (Discord/Slack)
- Supports multi-server management from one window
- Runs in **portable mode** (data next to the exe) or **installed mode** (`%APPDATA%`)

---

## 2. Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  React 19 + TypeScript + Tailwind 4 + Zustand (frontend)        │
│  src/ – components, features, hooks, store, lib                 │
├─────────────────────────────────────────────────────────────────┤
│  Tauri 2 IPC (commands + events)                                │
│  src/lib/ipc.ts ↔ src-tauri/src/commands.rs                     │
├─────────────────────────────────────────────────────────────────┤
│  Rust backend (tokio async)                                     │
│  src-tauri/src/                                                 │
│    state.rs       – AppState (instances, runtime, settings)     │
│    commands.rs    – #[tauri::command] handlers (thin wrappers)  │
│    core/          – business logic modules                      │
│      server.rs    – start/stop/restart/update lifecycle         │
│      steamcmd.rs  – SteamCMD execution + progress parsing       │
│      rcon.rs      – RCON TCP client                             │
│      mods.rs      – modlist.txt, pak inspection, conflicts      │
│      modsync.rs   – Workshop metadata, update checks, downloads │
│      access.rs    – blacklist.txt / whitelist.txt               │
│      backup.rs    – Server backup/restore                       │
│      supervisor.rs– Crash/freeze detection + auto-restart       │
│      proc.rs      – Process management (kill, scan, Ctrl+Break) │
│      ini.rs       – Lossless INI read/write (IniDoc)            │
│      secrets.rs   – DPAPI encryption for passwords/URLs         │
│      network.rs   – Port checks, UPnP                           │
│      history.rs   – Audit log, session tracking                 │
│      settings.rs  – Game rules (ServerSettings.ini) + enum map  │
│      gamedb.rs    – Game database queries                       │
│      diagnostics.rs– Health checks                              │
│      download.rs  – File download helper                        │
│      upnp.rs      – UPnP port forwarding                        │
└─────────────────────────────────────────────────────────────────┘
```

### Key design decisions

- **`commands.rs` is thin** – every `#[tauri::command]` delegates to `core::*`. Long-running work uses `background()` (spawns a tokio task, emits progress events).
- **`state.rs` owns all shared state** – `instances` (persisted), `runtime` (per-server live state), `rcon` (connections), `history` (audit log).
- **INI round-trips losslessly** – `IniDoc` in `ini.rs` preserves unknown keys, comments, ordering, repeated keys, and indexed keys. Never regex-replace INI text.
- **Secrets are DPAPI-encrypted** – `secrets.rs` `seal()`/`open()` handles RCON passwords and webhook URLs. Empty strings short-circuit (no-op).
- **Portable vs installed** – `paths.rs` checks for `portable.txt` next to the exe. If present, all data goes to `<exe_dir>/data/`. Otherwise `%APPDATA%\com.conanexiles.enhancedmanager\`.
- **Events flow one way** – Rust emits Tauri events (`task`, `status`, `log`, `backup-created`, etc.); React listens via `useBackendEvents.ts`.

---

## 3. Steam / game constants

| Constant | Value | Meaning |
|----------|-------|---------|
| `SERVER_APP_ID` | **443030** | Conan Exiles Dedicated Server (Enhanced/UE5) |
| `GAME_APP_ID` | **440900** | The game itself (Workshop mods live under this ID) |
| Branch `public` | – | Enhanced (UE5) server build |
| Branch `conan-exiles-legacy` | – | Old UE4 server (not the default) |
| `PROJECT` | `ConanSandbox` | Unreal project name (used in paths) |
| `SETTINGS_SECTION` | `ServerSettings` | INI section where all game rules live |
| `KNOWN_DEVKIT_REVISION` | `1002` | Fallback devkit revision for mod compatibility |

SteamCMD uses **anonymous login** (no Steam account needed). The app ships its own SteamCMD copy in the data directory.

---

## 4. Conventions

- **Rust:** `cargo check` must pass with zero warnings. Use `AppResult<T>` for fallible operations. All async work via tokio.
- **TypeScript:** `npx tsc -p tsconfig.json --noEmit` must pass. Use the Zustand store for state; don't create parallel state sources.
- **IPC:** All commands are camelCase on the wire. Types are mirrored between `models.rs` (Rust) and `src/lib/types.ts` (TS).
- **Events:** Event names are kebab-case strings (`task`, `status`, `log-line`, `backup-created`).
- **File writes:** Use `write_json_atomic` (backup + atomic rename) for any JSON persistence.
- **No TODO/FIXME/HACK** markers should be left in the codebase.

---

## 5. Build & test

```shell
# Prerequisites: Rust stable, Node 20+, Windows SDK
npm install

# Dev mode (hot reload)
npm run tauri dev

# Typecheck
npx tsc -p tsconfig.json --noEmit

# Rust check (warnings = errors in CI)
cd src-tauri && cargo check

# Rust tests
cd src-tauri && cargo test --lib

# Full release build (installer + portable)
npm run build:all
#   = tauri build (NSIS + MSI) + node scripts/build-portable.mjs --skip-build
```

**CI** (`.github/workflows/ci.yml`): typecheck → vite build → cargo test → cargo check with `-D warnings`.

**Release** (`.github/workflows/release.yml`): triggered by `v*` tags. Builds on `windows-latest`, publishes Setup.exe, .msi, and Portable.zip to GitHub Releases.

---

## 6. Gotchas & pitfalls

1. **`stop_locked` does a lot** – even with `force=true` it runs `apply_pending` and `backup_on_stop`. When deleting a server, bypass `server::stop` entirely and kill PIDs directly (see `delete_instance` in `commands.rs`).

2. **`running_pids` creates a new `sysinfo::System` each call** – it's called on user actions (not in a tight loop), so it's acceptable. If you add a polling loop, cache the `System` instance.

3. **SteamCMD progress parsing** – `steamcmd.rs` uses a regex (`PROGRESS_RE`) to parse percentage from stdout. The output format can change; the parser is defensive (ignores non-matching lines).

4. **RCON is TCP** – `Game.ini [RconPlugin] RconEnabled=1`, `RconPassword`, `RconPort` (default **25575**), `RconMaxKarma`. Replies carry the *previous* request id, so send one command at a time; bursts trip the karma limit ("Too many commands", then the IP is denied for ~1 min). `SetServerSetting` with a number for an **enum** setting is silently ignored even though the reply says "Setting X to N" – always send the enum *name* (`settings::live_value` does this).

5. **Mods are mounted in place** – `mods::write_modlist` writes absolute `.pak` paths from the shared SteamCMD Workshop cache into `ConanSandbox\Mods\modlist.txt` on every start (nothing is copied). Mod list changes while running apply on the next start.

6. **DPAPI is per-user** – encrypted secrets can only be decrypted by the same Windows user on the same machine. Don't copy the data folder to another user account.

7. **`devMock.ts`** – the frontend mock backend (`src/lib/devMock.ts`) must only load in browser dev mode (`!isTauri()`). It mirrors the real IPC API for Vite-only development.

8. **Port conflicts** – `start_locked` checks UDP game port and TCP RCON port before spawning. If another process owns the port, the start is refused with a clear error.

9. **The `background()` helper** in `commands.rs` spawns a tokio task and returns immediately. Errors become toast notifications. Use it for any command that might take >1s.

10. **`write_json_atomic`** reads the existing file first (to validate it's parseable JSON) before writing the backup. This prevents corrupting good data with a bad write.

---

11. **There is NO `DedicatedServer.ini`, Owner ID, world name, platform policy, beacon port or `.sav` world in Conan Exiles.** Those are Dragonwilds concepts that leaked in from the sister project in 1.0.0–1.0.4 and were removed in 1.0.5. The world is `Saved/game_0.db` (SQLite WAL). Ports: game UDP, game+1 UDP, query UDP (27015), RCON TCP. Profiles from those versions may still carry `ini.ownerId` etc. in `instances.json`; serde ignores them.

12. **Who owns which config value** (the root of the 1.0.4 "my setting got reset" reports):
    * **Game rules** (`ServerSettings.ini [ServerSettings]`, everything except the keys below) – the **file is the source of truth**. The server writes it itself: `SetServerSetting` (RCON *and* the in-game admin panel) writes through immediately, and a **clean shutdown rewrites the whole file from the server's memory** (verified: an external edit made while running is reverted by `shutdown`; Ctrl+Break does not rewrite). So: never cache rules in the profile, never write the file while the server runs, and never write a *whole-file snapshot* after it stops.
    * **Profile-owned keys** – `AdminPassword`, `ServerPassword`, `ServerMessageOfTheDay` (ServerSettings.ini), `ServerName` (Engine.ini), `ServerModList`, `[RconPlugin]`, `[Regions]`, `ServerDefaultMap`, the command line. Written by `server::sync_ini` on every start. Admins can change the MOTD/passwords in-game, so always go through **`server::sync_profile`**: it first calls `adopt_server_changes` (file ≠ last-written fingerprint *and* profile = last-written → the in-game value wins and is saved into the profile), then `mark_synced` (stores sha256 fingerprints in `ServerInstance.synced`), then `sync_ini`. Don't call bare `sync_ini` on a stored profile.
    * **Raw editor saves while running** (`write_config`) are stored as key-level `IniChange`s (`IniDoc::diff` against the text the editor loaded) and merged with `IniDoc::apply_changes` after the server exits (`apply_pending`). A save while stopped also merges when the file changed since the editor loaded it.
    * A **brand-new** ServerSettings.ini gets `ConfigVersion=13` / `BlueprintConfigVersion=25`; without them the server "upgrades from version 2" on first start and resets dozens of rules (seen in a real log).

13. **Game Rules catalog is generated** – `src/data/gameRules.ts` comes from `scripts/gen-game-rules.py` (labels, groups, descriptions, enum options) + `docs/research/enhanced-settings-facts.json` (verified keys/defaults). Edit the script, then run `python scripts/gen-game-rules.py`. Enum index order must match `core/settings.rs ENUMS`.

14. **Bans/whitelist** – `Saved/blacklist.txt` / `Saved/whitelist.txt`, lines `<id>[:<name>]` (CRLF). `whitelistplayer <id>` appends `<id>:Unknown`. RCON `banplayer (index|name|userid|platformid|player) <id> <msg>` only accepts players the server has seen, so offline bans are written to the file while the server is stopped (`core/access.rs`). Whitelisting is enforced only with `EnableWhitelist=True`.

---

## 7. Adding a new feature

1. **Define the type** in `models.rs` (Rust) and mirror in `src/lib/types.ts` (TS).
2. **Implement the logic** in the appropriate `core/` module.
3. **Add the command** in `commands.rs` (thin wrapper, use `background()` if async/long).
4. **Register it** in `lib.rs` → `tauri::generate_handler![...]`.
5. **Add the IPC call** in `src/lib/ipc.ts`.
6. **Add the mock** in `src/lib/devMock.ts` (for browser dev).
7. **Build the UI** in the appropriate `features/` or `components/` module.
8. **Verify:** `cargo check` (zero warnings), `npx tsc --noEmit`, `cargo test --lib`.

---

## 8. Testing without a real server

- The `devMock.ts` backend lets you run the frontend in a browser (`npx vite`) without Tauri.
- For backend testing, `cargo test --lib` runs unit tests.
- A real server install takes ~15 GB and 10+ minutes. The `.test-server/` and `.test-workshop/` folders (gitignored) are for local verification only – never commit them.

---

## 9. Release process

1. Bump version in `tauri.conf.json` + `package.json` + `Cargo.toml` (keep them in sync).
2. Update `CHANGELOG.md`.
3. Tag: `git tag v1.0.0` (or `v1.0.0-beta` for pre-releases).
4. Push the tag – GitHub Actions builds and publishes automatically.
5. Stable tags (no `-`) become `latest`; beta tags become pre-releases.
6. The README's download links use `/releases/latest/download/...` which always points to the latest stable.
