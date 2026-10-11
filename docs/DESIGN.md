# Conan Exiles Enhanced Server Manager — Design

> **CURRENT TARGET: CONAN EXILES: ENHANCED** (Unreal Engine 5, Steam dedicated server AppID **443030**, `public` branch).
> The UE4 game survives only as the `conan-exiles-legacy` beta branch. It's supported as an opt-in, and nothing is designed around it.

This document is the blueprint. [`AGENTS.md`](../AGENTS.md) is the hand-off guide (verified facts, invariants, how to continue).
[`ROADMAP.md`](ROADMAP.md) tracks what is built and what is next.

---

## 1. Product principles

1. **Never lose the user's data.** Hot backups of `game_0.db`, quarantine instead of delete, and history for every config write.
2. **The game is the source of truth.** Anything the server owns (`ServerSettings.ini`, `game_0.db`) is read from it and written back the way the server expects. While it runs, changes go through RCON. Nothing it writes is clobbered.
3. **Verify, don't assume.** Every game-specific behaviour is tested against a real Enhanced server and recorded in AGENTS.md §3. Unverified behaviour is labelled as such in code and docs.
4. **One tool for the whole job.** Install → configure → mod → run → watch → automate → recover, without ever opening an INI file by hand.
5. **Premium feel, zero ceremony.** Fast, animated, keyboard-first, beautiful in dark mode. Every destructive action can be undone or is confirmed.

## 2. Technology decisions

| Concern | Decision | Why |
|---|---|---|
| Shell | **Tauri 2** (WebView2) | ~10 MB native app, Rust backend, single-instance and tray. Same stack as the sibling managers, so their verified code is reused. |
| Backend | **Rust 2021 + tokio** | Process, file, network and SteamCMD work, plus one supervisor loop. |
| Frontend | **React 19 + TypeScript (strict) + Tailwind CSS 4 + Motion + Zustand** | As requested. |
| Components | **In-house kit in ShadCN style** (`src/components/ui`: `cn()`, variants, composable primitives) | ShadCN is a copy-in pattern, not a dependency. We own the components and keep the bundle small. |
| Data fetching | **Typed IPC + push events into Zustand** instead of React Query | The backend *pushes* state (status, telemetry, logs, tasks). React Query's pull cache would duplicate that. One-shot reads use a small `useBackend()` hook. |
| App config storage | **JSON files** (atomic write + `.bak` + corrupt-file quarantine) | Human-readable, diff-able, trivially portable. Profiles change rarely. |
| History, analytics, audit | **SQLite** (`manager.db`, rusqlite bundled, WAL) | Time series, player sessions, audit log, event timeline and mod cache: queryable, compact, crash-safe. |
| Game database | **Read-only SQLite** on `game_0.db`, plus **RCON `sql`** for live queries | Real clan and character data with no mods or hacks. |
| Logging | `tracing` → rolling file `data/logs/manager-YYYY-MM-DD.log` | Structured and greppable. |
| Secrets | **Windows DPAPI** (`CryptProtectData`, current-user scope) | Admin, RCON and server passwords, webhooks and API keys are never stored in plain text. |
| Charts | Hand-rolled SVG/canvas charts (from the sibling managers) | Zero dependencies, validated colours, downsampled to one point per pixel. |
| Editors | CodeMirror 6 | Raw INI, logs and modlist. |

## 3. Domain facts that shape the design (verified, see AGENTS.md §3)

- `ConanSandboxServer.exe` (root) is a launcher. The real work happens in `ConanSandbox\Binaries\Win64\ConanSandboxServer-Win64-Shipping.exe`. We launch the shipping exe directly, which keeps the PID honest.
- Ports: UDP game `7777`, UDP `game+1` (pinger), UDP query `27015`, TCP RCON `25575`, UDP `14001` (purpose unknown).
- **RCON is built in** (`Game.ini [RconPlugin]`) and is disabled until configured. The manager provisions it automatically with a random password. RCON gives:
  - clean `shutdown`;
  - live `SetServerSetting` / `GetServerSetting`, which also writes `ServerSettings.ini`;
  - `listplayers`, `kickplayer`, `banplayer`, `unbanplayer`, `whitelistplayer`;
  - `broadcast` and `sql <query>`.
- Conan's RCON replies carry the **previous** request's id. The client sends one command at a time and matches replies by order.
- Saves: `ConanSandbox\Saved\game_0.db` (SQLite, WAL). The server writes `game_0_backup_N.db` itself on every start.
- Settings: `ServerSettings.ini [ServerSettings]` (versioned: `ConfigVersion`, `BlueprintConfigVersion`), `Engine.ini` (`[OnlineSubsystem] ServerName/ServerPassword`, `[Regions] ExcludedRegions`), `Game.ini` (`[RconPlugin]`) and `ModHttp.ini` (per-mod HTTP origin approvals).
- Mods: `ConanSandbox\Mods\modlist.txt` lists absolute `.pak` paths in load order. Paks come from Workshop app **440900** (anonymous SteamCMD download works). Each pak embeds `modinfo.json` with `devkitRevisionNumber`, which must match the server (`1002` today), and `requestedHttpOrigins`.
- The map merges Exiled Lands and Isle of Siptah into **regions**, and regions can be excluded to save RAM.

## 4. Feature inventory

Legend: **P1** core (built first) · **P2** depth · **P3** polish/scale · **P4** platform. Status lives in ROADMAP.md.

### 4.1 Multi-server management
| Feature | Phase |
|---|---|
| Unlimited server profiles, each with its own install, settings, mods, backups, schedules and logs | P1 |
| Create (new install via SteamCMD) / import an existing install (auto-detect Steam library installs) | P1 |
| Clone (profile + configs ± saves) | P1 |
| Export/Import **server packs** (`.cesmpack`: profile + configs ± saves ± mod list, secrets optional) | P1 |
| Delete (backups moved out first, files optionally kept) | P1 |
| Favourites, colours, tags, notes, search | P1 |
| Folders/groups in the sidebar (collapsible, drag-reorder) | P2 |
| Archive (hide from the list, keep files) | P2 |
| Server templates (save a profile+configs as a template; built-in PvE / PvP / RP / Hardcore presets) | P2 |
| Move install to another drive (stop → move → re-root paths) | P2 |
| Bulk actions with multi-select (start/stop/restart/backup/update) | P2 |

### 4.2 Server control
| Feature | Phase |
|---|---|
| Start / Stop (graceful: RCON `shutdown` → Ctrl+Break → kill) / Restart / Force stop | P1 |
| Crash detection, auto-restart with delay, crash-loop guard | P1 |
| Adoption of servers started outside the manager; servers keep running when the manager closes | P1 |
| Hang detection (process alive but log silent and RCON unresponsive for N minutes) → smart recovery | P2 |
| Scheduled restarts with **in-game countdown broadcasts** (e.g. 15/5/1 min) | P1 |
| Launch profiles: ports, MaxPlayers, MULTIHOME/MULTIHOMEHTTP, catalog switches, extra args, live launch-line preview | P1 |
| Process priority, CPU affinity, memory/uptime restart limits | P1 |
| Region exclusion (Exiled Lands / Isle of Siptah) | P1 |

### 4.3 Installation management
| Feature | Phase |
|---|---|
| SteamCMD bootstrap, install/update/validate with live progress and retries with plain-language errors | P1 |
| Branch selection: `public` (Enhanced) / `conan-exiles-legacy` (UE4, warned) / custom beta + password | P1 |
| Build ids (installed vs latest) via api.steamcmd.net with a SteamCMD fallback; update checks | P1 |
| Installation integrity report (StateFlags, missing exe, stale Engine.ini build override) | P1 |
| Version history (build id per update recorded in manager.db) | P2 |
| Rollback = restore a pre-update backup + pin a branch (Steam can't download old builds anonymously) | P2 |
| Multiple install roots, migration tool | P2 |

### 4.4 Configuration system (never edit an INI by hand)
| Feature | Phase |
|---|---|
| **Schema-driven Game Rules editor** for every ServerSettings key: 219 documented keys plus the extra keys the server writes. Each key has a label, explanation, default, recommended value, type, range, unit, enum options and restart flag | P1 |
| Dependency awareness ("relies on PVPEnabled"): dependent fields dim and show why | P1 |
| Categories (Combat, XP, Harvest, Crafting, Thralls, Purge, Decay, Building, Followers, Avatars, Chat, Security, Clans, Events, NPCs, Maelstrom, Admin, Network…), search, Basic/Advanced/Expert modes | P1 |
| Per-weekday PvP / raid / avatar schedule editors (grids instead of 42 numeric keys) | P1 |
| Live apply while running (RCON `SetServerSetting`); file edit while stopped; queued if RCON is unavailable | P1 |
| Unknown keys preserved and shown under "Other keys" | P1 |
| Presets (PvE, PvE-Conflict, PvP, RP, Hardcore, Boosted) with a diff preview before applying | P1 |
| Engine.ini / Game.ini editors for the keys we know, plus raw editors (CodeMirror) for all four files | P1 |
| Config history: every write versioned in `_history`; diff and restore | P1 (history) · P2 (diff UI) |
| Side-by-side config diff between two servers | P2 |

### 4.5 Mod management
| Feature | Phase |
|---|---|
| Add by Workshop ID/URL, or import a whole **Workshop collection** | P1 |
| Shared Workshop cache (each mod downloaded once for all servers), anonymous SteamCMD | P1 |
| Metadata from the Steam API (title, preview, size, updated, tags) **and** embedded `modinfo.json` (author, version, devkit, origins) | P1 |
| Enable/disable, remove, **drag-and-drop load order** → manager-owned `modlist.txt` + `ServerModList` | P1 |
| **Compatibility check**: `Legacy` tag, `minimumVersion`, devkit revision vs the server's | P1 |
| Update detection (Workshop `time_updated` vs local), update all / on restart / automatic | P1 |
| **Conflict detection** from the server log (two mods overriding the same data-table row) | P2 |
| HTTP origin approvals UI (`ModHttp.ini`) fed by each mod's `requestedHttpOrigins` | P1 |
| Mod profiles (named mod sets per server, one-click switch) | P2 |
| Mod version history: previous pak archived before each update → rollback | P2 |
| Health score per mod (compatibility, freshness, load errors seen in the log) | P2 |
| In-app Workshop browser (needs a free Steam Web API key) | P3 |

### 4.6 Backups
| Feature | Phase |
|---|---|
| **Hot, consistent `game_0.db` snapshots** via the SQLite online-backup API (safe while running) | P1 |
| Include configs, mod list ± mod paks, logs; zip with manifest; SHA-256 checksums; integrity check on create | P1 |
| Scheduled/interval backups, before update, on stop; retention by count | P1 |
| One-click restore (current files quarantined first) | P1 |
| Retention by age, GFS (daily/weekly/monthly) | P2 |
| Point-in-time timeline view | P2 |
| Full-install backup | P3 |
| Cloud sync targets (folder sync first; S3/Drive via plugins) | P4 |

### 4.7 Monitoring
| Feature | Phase |
|---|---|
| Live CPU, RAM, disk IO, network, uptime, players, restart count | P1 |
| Health score (process, RCON, ports, crash history, memory trend, update/mod state) | P1 |
| History persisted in manager.db (1-minute rollups) with charts over 1 h / 24 h / 7 d | P2 |
| Database growth tracking (`game_0.db` size) | P2 |
| Server FPS/tick (RCON `dumpticks` parsing), alert conditions, predictive memory trend | P3 |

### 4.8 Players
| Feature | Phase |
|---|---|
| Live players (RCON `listplayers`: character, player, user id, platform) | P1 |
| Kick / ban / unban / whitelist with reason; broadcast | P1 |
| Player directory from `game_0.db`: characters, level, clan, last online, alive or killed by | P1 |
| Clans: members, owner, MOTD | P1 |
| Sessions and playtime (manager.db, from join/leave diffs) | P2 |
| Admin notes, warning/kick/ban history per player | P2 |
| Event log browser (`game_events`: kills, destruction, building) | P2 |

### 4.9 Administration center
| Feature | Phase |
|---|---|
| RCON console with history, autocomplete from `help`, saved commands | P1 |
| Unified console stream (server log + manager + SteamCMD + RCON), colour-coded and filterable | P1 |
| Command macros (sequences with delays/variables) | P2 |
| SQL console against `game_0.db` (read-only, with templates) | P2 |

### 4.10 Logging platform
| Feature | Phase |
|---|---|
| Live tail with level colouring, search, filters, pause | P1 |
| Archived logs (`ConanSandbox-backup-*.log`), open/search/export | P1 |
| Crash analysis (fatal lines, `Saved/Crashes` folders, last N lines before exit) | P2 |
| Audit log of every mutating manager action (manager.db) + event timeline | P1 (record) · P2 (UI) |

### 4.11 Automation engine
| Feature | Phase |
|---|---|
| Schedules (time + weekdays → restart / update / backup / stop / start / RCON command / broadcast) | P1 |
| Rules: **trigger → conditions → actions** workflows | P2 |
| Triggers: schedule, crash, server up/down, player join/leave, player-count threshold, update available, mod update available, metric threshold, log pattern | P2 |
| Actions: start/stop/restart (with countdown), backup, update server/mods, RCON command, broadcast, notify, webhook | P2 |

### 4.12 Notifications
| Feature | Phase |
|---|---|
| In-app toasts + task tray, desktop notifications | P1 |
| Discord/Slack webhooks with per-level routing | P1 |
| Rich Discord embeds, per-server routing, quiet hours | P2 |
| Email (SMTP) | P3 |

### 4.13 UI / UX
| Feature | Phase |
|---|---|
| Custom title bar, glass surfaces, ember/bronze theme, motion, command palette (Ctrl+K), keyboard shortcuts, tabs | P1 |
| Dashboard with server cards (status, players, CPU/RAM sparkline, quick actions) | P1 |
| Theme engine (accents, density/compact mode, reduce motion), streamer mode (masks IPs/passwords/IDs) | P2 |
| Split view / side-by-side servers, saved layouts | P3 |
| Detached windows (multi-monitor), workspace profiles | P3 |
| Localisation (string extraction + language packs) | P3 |

### 4.14 Platform
| Feature | Phase |
|---|---|
| Auto-updater (signed GitHub releases, tauri-plugin-updater) | P3 |
| Local REST + WebSocket API (localhost, token auth, same command layer) | P4 |
| Plugin system (see §9) | P4 |
| Roles and permissions (needed once remote/API users exist) | P4 |
| Web dashboard (served by the API) | P4 |

## 5. Architecture

```
┌──────────────────────────── React UI (WebView2) ─────────────────────────────┐
│ App shell: TitleBar · Sidebar · TabBar · CommandPalette · Toasts · TaskTray  │
│ features/dashboard · features/server/{views…} · features/dialogs             │
│ store/app.ts (Zustand) ◄── events ── lib/ipc.ts (typed invoke + listen)      │
└───────────────▲───────────────────────────────────────────────┬──────────────┘
                │ events: server-status, telemetry, log-lines,  │ invoke(cmd)
                │ task, server-event, player-list, …            ▼
┌──────────────────────────── Rust core (tokio) ───────────────────────────────┐
│ commands.rs  (thin #[tauri::command] layer, audit-logged)                    │
│   │                                                                          │
│   ├─ core/server.rs      lifecycle, launch line, INI sync, update jobs       │
│   ├─ core/supervisor.rs  ONE loop: processes, telemetry, log tail, RCON poll,│
│   │                      players, crash restart, schedules, backups, updates │
│   ├─ core/rcon.rs        Conan RCON client (ordered replies, reconnect)      │
│   ├─ core/settings.rs    ServerSettings schema + live/file apply             │
│   ├─ core/ini.rs         lossless Unreal INI document                        │
│   ├─ core/mods.rs        workshop cache, modinfo.json, modlist, compat       │
│   ├─ core/workshop.rs    Steam Web API (details, collections)                │
│   ├─ core/steamcmd.rs    bootstrap, app_update, workshop_download_item       │
│   ├─ core/backup.rs      hot DB snapshot, zip, checksums, restore, packs     │
│   ├─ core/gamedb.rs      read-only game_0.db queries (players, clans, events)│
│   ├─ core/diagnostics.rs health checks + one-click fixes                     │
│   ├─ core/network.rs     firewall rules, IPs, port owners                    │
│   ├─ core/proc.rs        detached launch, Ctrl+Break helper, priority, scan  │
│   ├─ core/secrets.rs     DPAPI encrypt/decrypt                               │
│   └─ core/history.rs     manager.db (metrics, sessions, audit, events)       │
│ state.rs  AppState (settings, instances, runtime map, locks, http, db)       │
│ models.rs persisted + event types (camelCase, mirrored in src/lib/types.ts)  │
└──────────────────────────────────────────────────────────────────────────────┘
         │ spawn (detached, own console)        │ TCP 25575          │ files
         ▼                                       ▼                    ▼
  ConanSandboxServer-Win64-Shipping.exe  ◄── RCON ──┘     Saved/*.ini, game_0.db, Logs, Mods
```

**Rules:**
- Commands are thin; logic lives in `core/*`.
- Every lifecycle operation for a server holds `AppState::op_lock(id)`. The supervisor only `try_lock`s.
- There is exactly one background loop (the supervisor). No per-server threads.
- Long jobs report through `TaskReporter` (progress plus cancel) and never block the UI.
- The UI never touches files. It asks the backend.

### 5.1 Graceful stop
1. If RCON is reachable, send `shutdown` and disconnect immediately. The server waits for RCON clients to drop. It then saves, checkpoints the DB and exits (~5–10 s).
2. Otherwise send Ctrl+Break via the helper process (`--send-ctrl-break <pid>`). The engine exits in about 2 s and the DB stays consistent through its WAL.
3. If the server still runs after `stop_timeout_secs`, kill the process tree.

### 5.2 Settings write path
```
UI edit ──► stopped?  ── yes ─► IniDoc set key (lossless) ─► write_file (history copy) ──► done
              │ no
              ▼
        RCON alive? ── yes ─► SetServerSetting k v  (server persists to file itself)
              │ no
              ▼
        queue in Runtime.pending_settings ─► applied on next stop/exit
```
Keys flagged "requires restart" are applied the same way and mark the server `pendingRestart`.

## 6. Data model

### 6.1 JSON (data dir)
- `settings.json`: `AppSettings` (paths, telemetry interval, webhooks, theme, Steam Web API key [DPAPI], streamer mode…).
- `instances.json`: `ServerInstance[]` (see `models.rs`). Profile-owned fields only: identity, network/launch, RCON, regions, mods + load order, mod profiles, automation, UI metadata (colour, icon, tags, folder, favourite, archived, notes). **Game rules are not stored in the profile.** They live in `ServerSettings.ini`.
- `folders.json`: sidebar groups.
- `templates/*.json`: server templates.

### 6.2 SQLite `manager.db`
```sql
CREATE TABLE schema_version (version INTEGER NOT NULL);

-- 1-minute rollups of telemetry per server
CREATE TABLE metrics (
  server_id TEXT NOT NULL, ts INTEGER NOT NULL,          -- unix ms, minute-aligned
  cpu REAL, mem INTEGER, read_bps INTEGER, write_bps INTEGER,
  players INTEGER, db_size INTEGER,
  PRIMARY KEY (server_id, ts)
);

-- Player sessions derived from RCON listplayers diffs
CREATE TABLE sessions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  server_id TEXT NOT NULL, user_id TEXT NOT NULL, platform_id TEXT,
  player_name TEXT, char_name TEXT,
  joined_at INTEGER NOT NULL, left_at INTEGER
);
CREATE INDEX sessions_user ON sessions(server_id, user_id);

-- Per-player admin data
CREATE TABLE player_notes (
  server_id TEXT NOT NULL, user_id TEXT NOT NULL,
  note TEXT NOT NULL DEFAULT '', updated_at INTEGER NOT NULL,
  PRIMARY KEY (server_id, user_id)
);
CREATE TABLE moderation (                                 -- warn | kick | ban | unban | whitelist
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  server_id TEXT NOT NULL, user_id TEXT NOT NULL, action TEXT NOT NULL,
  reason TEXT, actor TEXT NOT NULL DEFAULT 'admin', ts INTEGER NOT NULL
);

-- Everything that happened: lifecycle, crashes, updates, backups, mod changes
CREATE TABLE events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  server_id TEXT, ts INTEGER NOT NULL, level TEXT NOT NULL,   -- info|success|warn|error
  kind TEXT NOT NULL, message TEXT NOT NULL, data TEXT        -- JSON
);
CREATE INDEX events_ts ON events(server_id, ts);

-- Every mutating command (who/what/when)
CREATE TABLE audit (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts INTEGER NOT NULL, actor TEXT NOT NULL, server_id TEXT,
  action TEXT NOT NULL, detail TEXT
);

-- Build history (installed builds over time)
CREATE TABLE builds (
  server_id TEXT NOT NULL, build_id TEXT NOT NULL, branch TEXT NOT NULL, ts INTEGER NOT NULL
);

-- Workshop metadata cache
CREATE TABLE workshop_items (
  id TEXT PRIMARY KEY, title TEXT, preview_url TEXT, file_size INTEGER,
  time_updated INTEGER, tags TEXT, fetched_at INTEGER NOT NULL
);
```
Retention: metrics are kept 30 days by default. Everything else is kept forever. A Vacuum runs monthly.

### 6.3 The game's own database (read-only)
`account(id, user, online, platformId)` · `characters(playerId, id, char_name, level, rank, guild, isAlive, killerName, lastTimeOnline, killerId, lastServerTimeOnline)` · `guilds(guildId, name, messageOfTheDay, owner…)` · `game_events(…)` · `serverPopulationRecordings` · `dw_settings(name,value)`.

## 7. Folder structure

```
ConanExilesServerManager/
├─ AGENTS.md  CLAUDE.md  README.md  CHANGELOG.md  LICENSE
├─ docs/  DESIGN.md  ROADMAP.md  USER_GUIDE.md  research/ (source notes)
├─ scripts/  build-portable.mjs  gen-settings-schema.mjs
├─ src-tauri/
│  ├─ Cargo.toml  tauri.conf.json  capabilities/  icons/
│  └─ src/
│     ├─ main.rs (entry + helper modes)  lib.rs (setup, window, tray, command list)
│     ├─ commands.rs  state.rs  models.rs  paths.rs  error.rs
│     └─ core/  server supervisor rcon settings ini mods workshop steamcmd backup
│               gamedb diagnostics network proc secrets history download
├─ src/
│  ├─ main.tsx  App.tsx  index.css
│  ├─ lib/       ipc.ts types.ts format.ts ini.ts live.ts devMock.ts net.ts shell.ts
│  ├─ data/      settingsSchema.ts (generated) launchCatalog.ts presets.ts
│  ├─ store/     app.ts
│  ├─ hooks/     useBackendEvents.ts useServerActions.ts useBackend.ts
│  ├─ components/ ui/ (kit) layout/ (shell) charts/
│  └─ features/
│     ├─ dashboard/
│     ├─ dialogs/     NewServer, Import/Export/Clone/Delete, AppSettings
│     └─ server/      Workspace + views/ Overview Performance Players Console
│                     GameRules ServerSetup Mods Backups Automation Logs Config
│                     Health Network
└─ .test-server/ (git-ignored real install for verification)
```

## 8. UI

### 8.1 Navigation flow
```
Launch ─► Dashboard (all servers)
            ├─ [+ New server] ─► Wizard: name → install folder → branch → ports/identity → install
            ├─ [Import] ─► existing folder | Steam library install | .cesmpack
            └─ click card / sidebar ─► Server tab (Workspace)
                    Overview · Performance · Players · Console · Game Rules · Server Setup
                    Mods · Backups · Automation · Logs · Config Files · Health · Network
Ctrl+K anywhere ─► command palette (jump to server/view, run start/stop/backup, open settings)
```

### 8.2 Wireframes
```
┌─────────────────────────────────────────────────────────────────────────────────────┐
│ ⚔ Conan Exiles Enhanced Server Manager      [Ctrl+K search…]            _ ▢ ✕      │ TitleBar
├──────────────┬──────────────────────────────────────────────────────────────────────┤
│ ★ FAVOURITES │ [Home] [Siptah PvP ●] [RP Server ○] [+]                             │ TabBar
│ ● Siptah PvP │──────────────────────────────────────────────────────────────────────│
│ ○ RP Server  │ ┌ Siptah PvP ───────────── ● Running 3h12m · 12/40 · build 25792439 ┐│ Header
│ ▾ TESTING    │ │ [■ Stop] [↻ Restart ▾] [⇪ Update] [⛁ Backup]      CPU ▁▃▅ RAM ▂▃▃   ││
│ ○ Test 1     │ └──────────────────────────────────────────────────────────────────────┘│
│              │ Overview  Performance  Players  Console  Game Rules  Server Setup     │ View nav
│              │ Mods  Backups  Automation  Logs  Config  Health  Network             │
│              │ ┌───────────────────────────────┐ ┌──────────────────────────────┐   │
│ [+ New]      │ │ Status / connect info / ports │ │ Players online (live)        │   │
│ [⇩ Import]   │ ├───────────────────────────────┤ ├──────────────────────────────┤   │
│ ⚙ Settings   │ │ Health score 94  ✓ ✓ ✓ ! ✓    │ │ Recent events timeline       │   │
└──────────────┴─┴───────────────────────────────┴─┴──────────────────────────────┴───┘

Game Rules
┌──────────────┬───────────────────────────────────────────────────────┬──────────────┐
│ 🔍 search    │ Harvesting                                    Basic ▾ │ Presets      │
│ Server       │ Harvest Amount Multiplier        [====●=====] 2.0  ⟲ │  PvE         │
│ PvP & Raids  │   Multiplies resources gathered. Default 1 · Rec 2-3  │  PvP         │
│ XP           │ Resource Respawn Speed           [==●=======] 1.0    │  Roleplay    │
│ Harvesting ● │ Building Damage   (needs PvP)    dimmed               │  Hardcore    │
│ Crafting     │                                                        │ Changes (3)  │
│ Thralls      │                                    [Discard] [Apply ⚡ live]         │
└──────────────┴───────────────────────────────────────────────────────┴──────────────┘

Mods
┌───────────────────────────────────────────────────────────────────────────────────┐
│ [+ Add by ID/URL] [⇩ Import collection] [⟳ Check updates]      3 enabled · 1 update│
│ ⋮⋮ 1 [img] Pippi (Enhanced)      v3.10  ✓ devkit 1002   ⚠ update   [on] [⋯]       │
│ ⋮⋮ 2 [img] Growth Rates 100      v1.0.10 ✓              ✓ current  [on] [⋯]       │
│ ⋮⋮ 3 [img] Old UI Mod            ✕ Legacy (UE4) – will not load   [off][⋯]        │
│ Conflicts: Growth Rates 100 ↔ Thrall Rep both override NPCStatsAndGrowths:Bear    │
│ HTTP access requests: Voice Range → https://api.example.com [Allow] [Deny]        │
└───────────────────────────────────────────────────────────────────────────────────┘
```

### 8.3 Visual language
- **Palette:** warm basalt surfaces (`#0b0908…#231d19`), a bronze/ember accent (`#f59e0b → #ea580c`), blood-red danger and sandstone text. The Hyborian mood without kitsch.
- **Depth:** glass panels (`backdrop-blur`), 1px inner highlights, soft shadows, an ember glow for the primary action.
- **Motion:** 120–200 ms ease-out transitions, a layout spring for reordering, and a shimmer on progress. Reduce-motion is honoured.
- **Type:** Inter Variable for UI and JetBrains Mono for consoles and paths.

## 9. Plugin architecture (P4, designed now so the core stays compatible)

- **Manifest** `plugin.json`: id, version, author, permissions (`servers.read`, `servers.control`, `rcon`, `files.read:<glob>`, `net:<origin>`), contributions (views, dashboard widgets, commands, automation triggers/actions, themes).
- **UI contributions** run in a sandboxed `<iframe>` (no Tauri IPC) and talk to the host over `postMessage` through a capability-checked bridge. That bridge is the same typed API as `lib/ipc.ts`, filtered by the granted permissions.
- **Backend contributions** are WASM components (wasmtime) with host functions that mirror the command layer. They have no raw filesystem or network access beyond the granted permissions. Native DLL plugins are deliberately excluded because one bad plugin could compromise the machine.
- **Themes** are pure data (CSS variable sets) and need no permissions.
- **Distribution:** `.cesmplugin` zip, signature optional; a marketplace index can come later.
- **Stable seams the core already provides:** the event bus (`app.emit` names), the command layer, the automation trigger/action enums and the view registry (`features/server/views.ts`).

## 10. Security

- Secrets (admin, RCON and server passwords, webhooks, Steam API key) are encrypted with DPAPI at rest. They never go on the command line (visible to every process) and are never logged.
- RCON is bound by the game to `0.0.0.0`. The manager never opens TCP 25575 in the firewall and warns if it's reachable from outside.
- Every mutating command is written to the audit table.
- File operations are confined to the server folder (`proc::is_within`). Zip extraction refuses path escapes.
- Destructive operations quarantine first (`Saved/_quarantine/<stamp>-<label>`).
- Portable mode: DPAPI secrets don't survive a move to another Windows account or PC. The UI detects undecryptable values and asks the user to re-enter them, and nothing is lost silently.

## 11. Quality bar

- `cargo check` warning-free (CI `-D warnings`), `cargo test --lib`, `npx tsc --noEmit`, `vite build`.
- Every parser of game output (log lines, RCON replies, modinfo, INI) has unit tests built from **real** captured samples.
- The mock backend (`devMock.ts`) mirrors every command, so the UI can be developed in a browser.
