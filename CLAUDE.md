# CLAUDE.md

All project guidance for AI assistants lives in **[AGENTS.md](AGENTS.md)** – read it before making changes.

Quick reminders:
- Steam AppID for the dedicated server is **443030** (Enhanced/UE5, branch `public`). Game AppID for Workshop mods is **440900**.
- Never regex-replace INI text; use `IniDoc` (`ini.rs`) for lossless round-trips.
- Keep `cargo check` warning-free, `cargo test --lib` and `npx tsc -p tsconfig.json --noEmit` passing.
- The mock backend (`src/lib/devMock.ts`) must only load in browser dev (`!isTauri()`).
- `commands.rs` is thin – delegate to `core/*`. Use `background()` for anything >1s.
- Secrets (RCON passwords, webhook URLs) are DPAPI-encrypted via `secrets.rs`. Never store them in plaintext.
- Portable mode is detected by `portable.txt` next to the exe (`paths.rs`). All data goes to `<exe_dir>/data/`.
