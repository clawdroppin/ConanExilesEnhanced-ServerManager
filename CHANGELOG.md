# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.5] – 2026-10-13

### Fixed
- **In-game admin changes are no longer overwritten on launch.** The manager rewrote the message of the day and passwords from its profile on every start, wiping values changed in-game as admin (e.g. the MOTD). It now detects in-game changes and keeps them, saving them into the profile (`server::sync_profile`).
- **Config Files saves made while the server was running** used to be written back as a whole-file snapshot after it stopped, silently reverting anything changed in-game in the meantime (drop-loot-on-death, rates…). They are now merged key-by-key; saves while stopped merge too if the file changed since it was opened.
- **Config Files "Save" always showed an error** even though the file was saved (the frontend expected a field the backend never returned).
- **Live enum settings were silently ignored**: values like drop-on-death or server region were sent to RCON as numbers, which the server ignores even though it replies "Setting … to …". They are now sent by name.
- **First start of a new server reset rules**: a freshly created ServerSettings.ini lacked `ConfigVersion`, so the server ran its settings upgrade and reset many rules, including preset values. New files now get the current versions.
- **Server Settings fields did nothing**: server name, join password and admin password edited fields that were only written to a `DedicatedServer.ini` Conan never reads. They now edit the real values.
- Removed the remaining Dragonwilds leftovers: Owner ID, world name, platform policy, beacon port (+1111), the `DedicatedServer.ini` file in the editor, backups and packs, and the claim that PvP/difficulty "aren't server config".
- **Port forwarding / firewall texts** now list Conan's real ports: game UDP, game + 1 UDP, query UDP (27015) and optional RCON TCP.
- **Players**: the ban, admin and known-player lists were always empty (wrong file and list names).

### Added
- **Game Rules view**: every `ServerSettings.ini` rule (≈290, including undocumented ones found in the binary) with search, groups, a "changed from default" filter, dependency hints, reset to default, quick presets (Relaxed PvE, Weekend raids, Small group boost) and an editor for unknown/mod keys. Changes apply live over RCON while the server runs, otherwise they are saved to the file. Admin changes made in-game show up there.
- **Players view rebuilt**: online players with warn/kick/ban, every character in the world with search and ban/whitelist buttons, ban list (`Saved/blacklist.txt`) and whitelist (`Saved/whitelist.txt`) with add/remove (RCON while running, file while stopped), an "Enforce whitelist" toggle and a moderation log.
- **Server Settings**: RCON section (enable, port, password, karma limit, player-list refresh), "Skip map regions" (Exiled Lands / Isle of Siptah), HTTP bind IP (`-MULTIHOMEHTTP`), query port, and "Free ports" now also picks a free query port.
- **New server wizard**: choose PvE/PvP and what players drop on death (keep everything / backpack / everything) up front.

## [1.0.4] – 2026-10-11

### Added
- **MOTD field** in Settings → Listing – set the server's Message of the Day (applied live via RCON while running)
- **Server stats card** on the Overview tab – total playtime, longest session, unique players, session count and first/last seen, tracked across restarts
- **BattlEye anticheat toggle** in Settings → Listing – start the server without anticheat (writes `IsBattlEyeEnabled` to `ServerSettings.ini`, applies on next start)

### Fixed
- **Config sync**: `DedicatedServer.ini` values were being written to the wrong INI section and silently lost on save – now read and written under the correct `/Script/Dominion.DedicatedServerSettings` section
- **Console**: duplicated RCON send logic consolidated – fewer edge cases when sending commands quickly

## [1.0.3] – 2026-10-11

### Fixed
- **Worlds tab**: world list now populates from the correct path; delete fully removes the saved world (quarantined for recovery)
- **Config tabs**: "unknown config file" error resolved – all 5 real config files (including `DedicatedServer.ini`) are now recognized
- **Mod paste**: added retry on transient Steam API failures so the first paste no longer silently skips mods

### Added
- **Server region** dropdown in Settings (Europe / North America / Asia / Australia / South America / Japan) – writes `serverRegion` to `[ServerSettings]`

## [1.0.2] – 2026-10-11

### Added
- **Workshop mod browser** on the Mods tab – search the Steam Workshop for Conan Exiles mods and install them with one click (needs a free Steam Web API key, added in Settings)
- **Paste Workshop ids/links** to install mods (one per line, or treat each as a collection) – works without an API key
- **Update** button on the Mods tab to pull the latest versions of installed mods (applied on next start)
- **Steam Web API key** field in Settings (stored encrypted)

### Fixed
- “Add local .pak” now uses the correct local-mod command (previously failed with “No Workshop id or link found”)

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
