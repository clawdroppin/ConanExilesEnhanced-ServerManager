//! Server lifecycle: launch-line construction, INI sync, start / stop / restart / update.
//!
//! Every lifecycle operation for one server runs under `AppState::op_lock(id)` so a scheduled
//! restart, a crash restart and a button click can never interleave.

use super::ini::{self, IniDoc};
use super::{backup, mods, proc, secrets, steamcmd};
use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::state::{manager_log, notify, now_ms, AppState, TaskReporter};
use std::time::Duration;
use tauri::AppHandle;

/// Switches the profile manages itself; ignored if they also appear in options / extra args.
const RESERVED: &[&str] = &["-port", "-queryport", "-multihome", "-multihomehttp", "-maxplayers", "-rconport", "-rconpassword"];

fn quote_if_needed(v: &str) -> String {
    if v.contains(' ') && !v.starts_with('"') {
        format!("\"{v}\"")
    } else {
        v.to_string()
    }
}

fn reserved(k: &str) -> bool {
    let base = k.split('=').next().unwrap_or(k).to_lowercase();
    RESERVED.contains(&base.as_str())
}

/// Exact command-line arguments after the executable. Passwords never go here (the command line
/// is readable by every process) – they live in the INI files.
pub fn build_args(inst: &ServerInstance) -> Vec<String> {
    let l = &inst.launch;
    let mut args = vec![format!("-Port={}", l.port), format!("-QueryPort={}", l.query_port), format!("-MaxPlayers={}", l.max_players)];
    if !l.multihome.trim().is_empty() {
        args.push(format!("-MULTIHOME={}", l.multihome.trim()));
        // Funcom: with MULTIHOME the server must also issue its HTTP (registration) requests from
        // that IP, or it won't appear in the server browser.
        let http = if l.multihome_http.trim().is_empty() { l.multihome.trim() } else { l.multihome_http.trim() };
        args.push(format!("-MULTIHOMEHTTP={http}"));
    } else if !l.multihome_http.trim().is_empty() {
        args.push(format!("-MULTIHOMEHTTP={}", l.multihome_http.trim()));
    }
    for (k, v) in &l.options {
        if reserved(k) {
            continue;
        }
        match v {
            LaunchValue::Flag(true) => args.push(k.clone()),
            LaunchValue::Flag(false) => {}
            LaunchValue::Value(s) if !s.trim().is_empty() => args.push(format!("{k}={}", quote_if_needed(s.trim()))),
            LaunchValue::Value(_) => {}
        }
    }
    for extra in split_args(&l.extra_args) {
        if !reserved(&extra) {
            args.push(extra);
        }
    }
    args
}

/// Split a free-form argument string, keeping "quoted values" together.
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut in_q = false;
    for c in s.chars() {
        match c {
            '"' => {
                in_q = !in_q;
                cur.push(c);
            }
            c if c.is_whitespace() && !in_q => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub fn launch_line(inst: &ServerInstance) -> String {
    format!("\"{}\" {}", inst.server_exe().display(), build_args(inst).join(" "))
}

/// Write the profile-owned keys into the game's INI files. Everything else in those files is left
/// exactly as it is (lossless `IniDoc`): the server's own `ConfigVersion` keys, game rules, etc.
///
/// * ServerSettings.ini – AdminPassword, ServerPassword, ServerMessageOfTheDay, ServerModList
/// * Engine.ini – `[OnlineSubsystem] ServerName/ServerPassword`, `[Regions] ExcludedRegions`, and
///   removal of the stale UE4 `bUseBuildIdOverride/BuildIdOverride` lines Funcom says block joins
/// * Game.ini – `[RconPlugin]`
pub fn sync_ini(inst: &ServerInstance) -> AppResult<Vec<String>> {
    let mut notes = vec![];
    std::fs::create_dir_all(inst.config_dir())?;

    let path = inst.settings_path();
    let mut s = IniDoc::parse(&ini::read_file(&path)?);
    s.set(SETTINGS_SECTION, "AdminPassword", &inst.identity.admin_password);
    s.set(SETTINGS_SECTION, "ServerPassword", &inst.identity.server_password);
    s.set(SETTINGS_SECTION, "ServerMessageOfTheDay", &inst.identity.motd);
    s.set(SETTINGS_SECTION, "ServerModList", &mods::server_mod_list(inst));
    ini::write_file(&path, &s.to_text())?;

    let path = inst.engine_ini();
    let mut e = IniDoc::parse(&ini::read_file(&path)?);
    e.set("OnlineSubsystem", "ServerName", inst.identity.server_name.trim());
    e.set("OnlineSubsystem", "ServerPassword", &inst.identity.server_password);
    if e.remove("OnlineSubsystem", "bUseBuildIdOverride") | e.remove("OnlineSubsystem", "BuildIdOverride") {
        notes.push("Removed a stale UE4 BuildIdOverride from Engine.ini (it stops players from joining Enhanced servers).".into());
    }
    let excluded: Vec<&str> = inst.regions.excluded.iter().map(|s| s.as_str()).filter(|r| REGIONS.iter().any(|(k, _)| k == r)).collect();
    if excluded.is_empty() {
        e.remove("Regions", "ExcludedRegions");
    } else {
        e.set("Regions", "ExcludedRegions", &excluded.join(","));
    }
    // Map selection – `ServerDefaultMap`. Reuse the GameMapsSettings section however the game
    // spelled it (full `/Script/...` path or a short name); create the documented one if absent.
    // Empty = leave the game's default map in place (we drop the key so the game falls back).
    let map = inst.ini.default_map.trim();
    let maps_section = e
        .sections()
        .into_iter()
        .find(|s| s.to_ascii_lowercase().contains("gamemapssettings"))
        .unwrap_or_else(|| "/Script/EngineSettings.GameMapsSettings".to_string());
    if map.is_empty() {
        e.remove(&maps_section, "ServerDefaultMap");
    } else {
        e.set(&maps_section, "ServerDefaultMap", map);
    }
    ini::write_file(&path, &e.to_text())?;

    let path = inst.game_ini();
    let mut g = IniDoc::parse(&ini::read_file(&path)?);
    let r = &inst.rcon;
    g.set("RconPlugin", "RconEnabled", if r.enabled && !r.password.is_empty() { "1" } else { "0" });
    g.set("RconPlugin", "RconPassword", &r.password);
    g.set("RconPlugin", "RconPort", &r.port.to_string());
    g.set("RconPlugin", "RconMaxKarma", &r.max_karma.to_string());
    ini::write_file(&path, &g.to_text())?;

    // DedicatedServer.ini – owner/world/platform settings.
    let path = inst.dedicated_ini();
    let mut d = IniDoc::parse(&ini::read_file(&path).unwrap_or_default());
    const DS: &str = "/Script/Dominion.DedicatedServerSettings";
    if !inst.ini.owner_id.is_empty() {
        d.set(DS, "OwnerId", &inst.ini.owner_id);
    }
    if !inst.ini.server_name.is_empty() {
        d.set(DS, "ServerName", &inst.ini.server_name);
    }
    if !inst.ini.default_world_name.is_empty() {
        d.set(DS, "DefaultWorldName", &inst.ini.default_world_name);
    }
    if !inst.ini.admin_password.is_empty() {
        d.set(DS, "AdminPassword", &inst.ini.admin_password);
    }
    d.set(DS, "WorldPassword", &inst.ini.world_password);
    if !inst.ini.platform_policy.is_empty() {
        d.set(DS, "PlatformPolicy", &inst.ini.platform_policy);
    }
    d.set(DS, "bAllowSendingCrashDumps", if inst.ini.allow_crash_dumps { "True" } else { "False" });
    ini::write_file(&path, &d.to_text())?;

    Ok(notes)
}

/// Read the manager-owned values from an existing install into a profile (used on import).
pub fn read_ini_into(inst: &mut ServerInstance) {
    let s = IniDoc::parse(&ini::read_file(&inst.settings_path()).unwrap_or_default());
    if let Some(v) = s.get(SETTINGS_SECTION, "AdminPassword") {
        inst.identity.admin_password = v;
    }
    if let Some(v) = s.get(SETTINGS_SECTION, "ServerPassword") {
        inst.identity.server_password = v;
    }
    if let Some(v) = s.get(SETTINGS_SECTION, "ServerMessageOfTheDay") {
        inst.identity.motd = v;
    }
    let e = IniDoc::parse(&ini::read_file(&inst.engine_ini()).unwrap_or_default());
    if let Some(v) = e.get("OnlineSubsystem", "ServerName").filter(|v| !v.trim().is_empty()) {
        inst.identity.server_name = v.trim().trim_matches('"').to_string();
    }
    if inst.identity.server_password.is_empty() {
        if let Some(v) = e.get("OnlineSubsystem", "ServerPassword") {
            inst.identity.server_password = v;
        }
    }
    if let Some(v) = e.get("Regions", "ExcludedRegions") {
        inst.regions.excluded = v.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
    }
    if let Some(sec) = e.sections().into_iter().find(|s| s.to_ascii_lowercase().contains("gamemapssettings")) {
        if let Some(v) = e.get(&sec, "ServerDefaultMap").filter(|v| !v.trim().is_empty()) {
            inst.ini.default_map = v.trim().to_string();
        }
    }
    if let Some(p) = e.get("URL", "Port").and_then(|v| v.trim().parse().ok()) {
        inst.launch.port = p;
    }
    let g = IniDoc::parse(&ini::read_file(&inst.game_ini()).unwrap_or_default());
    if let Some(v) = g.get("RconPlugin", "RconPassword").filter(|v| !v.is_empty()) {
        inst.rcon.password = v;
    }
    if let Some(p) = g.get("RconPlugin", "RconPort").and_then(|v| v.trim().parse().ok()) {
        inst.rcon.port = p;
    }
    if let Some(v) = g.get("RconPlugin", "RconEnabled") {
        let on = v.trim() == "1" || v.trim().eq_ignore_ascii_case("true");
        // A server we adopt keeps RCON on (the manager needs it); a fresh password is generated if
        // none exists.
        inst.rcon.enabled = on || inst.rcon.enabled;
    }
    if inst.rcon.password.is_empty() {
        inst.rcon.password = secrets::random_password(24);
    }
    if inst.mods.is_empty() {
        if let Ok(text) = std::fs::read_to_string(inst.modlist_path()) {
            inst.mods = mods::parse_modlist(&text, &inst.mods_dir());
        }
    }
    // Read DedicatedServer.ini for owner/world settings.
    let d = IniDoc::parse(&ini::read_file(&inst.dedicated_ini()).unwrap_or_default());
    const DS: &str = "/Script/Dominion.DedicatedServerSettings";
    if let Some(v) = d.get(DS, "OwnerId").filter(|v| !v.trim().is_empty()) {
        inst.ini.owner_id = v.trim().to_string();
    }
    if let Some(v) = d.get(DS, "ServerName").filter(|v| !v.trim().is_empty()) {
        inst.ini.server_name = v.trim().to_string();
    }
    if let Some(v) = d.get(DS, "DefaultWorldName").filter(|v| !v.trim().is_empty()) {
        inst.ini.default_world_name = v.trim().to_string();
    }
    if let Some(v) = d.get(DS, "AdminPassword").filter(|v| !v.trim().is_empty()) {
        inst.ini.admin_password = v.trim().to_string();
    }
    if let Some(v) = d.get(DS, "WorldPassword") {
        inst.ini.world_password = v.trim().to_string();
    }
    if let Some(v) = d.get(DS, "PlatformPolicy").filter(|v| !v.trim().is_empty()) {
        inst.ini.platform_policy = v.trim().to_string();
    }
    if let Some(v) = d.get(DS, "bAllowSendingCrashDumps") {
        inst.ini.allow_crash_dumps = v.trim() == "True" || v.trim().eq_ignore_ascii_case("true");
    }
}

/// PIDs of this server's running processes and the main (engine) PID.
pub fn running_pids(inst: &ServerInstance) -> (Vec<u32>, Option<u32>) {
    let mut sys = sysinfo::System::new();
    let all = proc::scan_servers(&mut sys);
    let mine = proc::procs_for_install(&all, &inst.install_dir);
    (mine.iter().map(|p| p.pid).collect(), proc::main_pid(&mine))
}

/// Write config files / game-rule changes that were saved while the server was running.
pub async fn apply_pending(state: &AppState, inst: &ServerInstance) -> AppResult<bool> {
    let (configs, settings) = state.with_runtime(&inst.id, |r| (std::mem::take(&mut r.pending_configs), std::mem::take(&mut r.pending_settings))).await;
    let any = !configs.is_empty() || !settings.is_empty();
    for (file, text) in configs {
        ini::write_file(&inst.config_dir().join(&file), &text)?;
    }
    if !settings.is_empty() {
        super::settings::write_file_values(inst, &settings)?;
    }
    Ok(any)
}

pub fn validate_for_start(inst: &ServerInstance) -> AppResult<Vec<String>> {
    let mut warnings = vec![];
    if inst.identity.server_name.trim().is_empty() {
        return Err(AppError::msg("Give the server a name first (Server Setup → Identity)."));
    }
    if inst.identity.admin_password.trim().is_empty() {
        warnings.push("No admin password is set – nobody can use admin commands in-game.".to_string());
    }
    if !inst.rcon.enabled {
        warnings.push("RCON is off: stops can't save cleanly, settings can't change live and players aren't listed.".to_string());
    }
    if inst.is_legacy() {
        warnings.push("This server runs the Legacy (UE4) branch – Enhanced clients can't join it.".to_string());
    }
    Ok(warnings)
}

pub async fn start(state: &AppState, app: &AppHandle, id: &str) -> AppResult<()> {
    let lock = state.op_lock(id).await;
    let _g = lock.lock().await;
    start_locked(state, app, id).await
}

async fn start_locked(state: &AppState, app: &AppHandle, id: &str) -> AppResult<()> {
    let mut inst = state.instance(id).await?;
    if !inst.is_installed() {
        return Err(AppError::msg("Server files are not installed yet. Click Install first."));
    }
    if let (_, Some(pid)) = running_pids(&inst) {
        state
            .with_runtime(id, |r| {
                r.pid = Some(pid);
                r.state = Some(ServerState::Running);
                r.started_at.get_or_insert(now_ms());
            })
            .await;
        state.emit_status(app, id).await;
        return Ok(());
    }
    // RCON is how the manager stops the server cleanly – make sure it has credentials.
    if inst.rcon.enabled && inst.rcon.password.is_empty() {
        inst.rcon.password = secrets::random_password(24);
        state.put_instance(&inst).await?;
    }
    for w in validate_for_start(&inst)? {
        notify(app, id, "warn", w);
    }

    // Port conflicts with something else on the machine.
    let owners = proc::udp_port_owners().await;
    for port in inst.udp_ports() {
        if let Some(pid) = owners.get(&port) {
            let mut sys = sysinfo::System::new();
            let name = proc::process_name(&mut sys, *pid).unwrap_or_else(|| "another program".into());
            return Err(AppError::msg(format!(
                "UDP port {port} is already in use by {name} (PID {pid}). Pick different ports under Server Setup → Network."
            )));
        }
    }
    if inst.rcon.enabled {
        if let Some(pid) = proc::tcp_listeners().await.get(&inst.rcon.port) {
            let mut sys = sysinfo::System::new();
            let name = proc::process_name(&mut sys, *pid).unwrap_or_else(|| "another program".into());
            return Err(AppError::msg(format!("TCP port {} (RCON) is already in use by {name} (PID {pid}). Pick another RCON port.", inst.rcon.port)));
        }
    }

    if inst.automation.update_on_start {
        state.set_busy(app, id, Some("Checking for updates")).await;
        let res = update_files(state, app, &inst, false).await;
        state.set_busy(app, id, None).await;
        if let Err(e) = res {
            notify(app, id, "warn", format!("Update before start failed, starting anyway: {e}"));
        }
    }

    // Mods: download anything missing (and updates, when enabled), then write modlist.txt.
    if inst.mods.iter().any(|m| m.enabled) {
        state.set_busy(app, id, Some("Preparing mods")).await;
        let res = super::modsync::sync(state, app, &inst, inst.automation.update_mods_on_start, true).await;
        state.set_busy(app, id, None).await;
        if let Err(e) = res {
            notify(app, id, "warn", format!("Mod download problem, starting with what's on disk: {e}"));
        }
    }
    let cache = state.workshop_dir().await;
    let missing = mods::write_modlist(&cache, &inst)?;
    if !missing.is_empty() {
        notify(app, id, "warn", format!("Starting without {} mod(s) that aren't on disk: {}", missing.len(), missing.join(", ")));
    }

    apply_pending(state, &inst).await?;
    for note in sync_ini(&inst)? {
        manager_log(app, id, note);
    }
    let args = build_args(&inst);
    let exe = inst.server_exe();
    // The engine rotates its log on start; remember where the old one ended so the tail only
    // streams the new session.
    let old_len = std::fs::metadata(inst.log_path()).map(|m| m.len()).unwrap_or(0);
    let pid = proc::spawn_server(&exe, &args, !inst.launch.show_console, &inst.launch.priority)?;
    if !inst.launch.affinity.is_empty() {
        let _ = proc::apply_priority_affinity(pid, &inst.launch.priority, &inst.launch.affinity);
    }
    state.rcon.disconnect(id).await;
    state
        .with_runtime(id, |r| {
            r.pid = Some(pid);
            r.state = Some(ServerState::Starting);
            r.started_at = Some(now_ms());
            r.ready_at = None;
            r.intentional_stop = false;
            r.pending_restart = false;
            r.restart_at = None;
            r.restart_pending_at = None;
            r.players.clear();
            r.rcon_ok = false;
            r.log_offset = old_len;
            r.log_partial.clear();
            r.last_log_at = now_ms();
            r.console_hidden = !inst.launch.show_console;
            r.log_ready = false;
            r.mods_loaded.clear();
            r.merging_mod = None;
            r.row_overrides.clear();
            r.http_requests.clear();
            r.engine_build = None;
            r.last_exit_code = None;
        })
        .await;
    state.history.audit(Some(id), "start", &args.join(" "));
    manager_log(app, id, format!("Launching: {} {}", exe.file_name().unwrap_or_default().to_string_lossy(), args.join(" ")));
    if inst.launch.upnp {
        let (app2, i2) = (app.clone(), inst.clone());
        tauri::async_runtime::spawn(async move {
            match super::upnp::map_server(&i2).await {
                Ok(lines) => manager_log(&app2, &i2.id, format!("UPnP: {}", lines.join(", "))),
                Err(e) => notify(&app2, &i2.id, "warn", format!("UPnP port forwarding failed: {e}")),
            }
        });
    }
    state.emit_status(app, id).await;
    Ok(())
}

/// Graceful stop: RCON `shutdown` (clean save + DB checkpoint) → Ctrl+Break → kill.
pub async fn stop(state: &AppState, app: &AppHandle, id: &str, force: bool) -> AppResult<()> {
    let lock = state.op_lock(id).await;
    let _g = lock.lock().await;
    stop_locked(state, app, id, force).await
}

async fn stop_locked(state: &AppState, app: &AppHandle, id: &str, force: bool) -> AppResult<()> {
    let inst = state.instance(id).await?;
    let (pids, main) = running_pids(&inst);
    let Some(main) = main else {
        state
            .with_runtime(id, |r| {
                r.pid = None;
                r.state = Some(ServerState::Stopped);
                r.restart_at = None;
                r.players.clear();
            })
            .await;
        state.rcon.disconnect(id).await;
        state.emit_status(app, id).await;
        return Ok(());
    };
    state
        .with_runtime(id, |r| {
            r.intentional_stop = true;
            r.restart_at = None;
            r.state = Some(ServerState::Stopping);
        })
        .await;
    state.emit_status(app, id).await;
    state.history.audit(Some(id), if force { "force-stop" } else { "stop" }, "");

    let timeout = inst.automation.stop_timeout_secs.clamp(10, 900) as u64;
    let wait_exit = |secs: u64| {
        let pids = pids.clone();
        async move {
            let mut sys = sysinfo::System::new();
            for _ in 0..secs * 2 {
                tokio::time::sleep(Duration::from_millis(500)).await;
                if !pids.iter().any(|p| proc::pid_alive(&mut sys, *p)) {
                    return true;
                }
            }
            false
        }
    };
    let mut exited = false;
    if !force {
        if inst.rcon.enabled && !inst.rcon.password.is_empty() {
            manager_log(app, id, "Asking the server to save and shut down (RCON shutdown)…");
            match state.rcon.shutdown_server(id, &AppState::rcon_target(&inst)).await {
                Ok(()) => exited = wait_exit(timeout).await,
                Err(e) => manager_log(app, id, format!("RCON shutdown unavailable ({e}) – falling back to a console stop.")),
            }
        }
        if !exited {
            manager_log(app, id, "Sending a console stop request (Ctrl+Break)…");
            if proc::request_graceful_exit(main).await {
                exited = wait_exit(timeout.min(60)).await;
            } else {
                manager_log(app, id, "The server has no console to signal (was -log removed?).");
            }
        }
        if !exited {
            manager_log(app, id, format!("Server did not exit in time – forcing it to close."));
            notify(app, id, "warn", format!("{} didn't shut down in time and was force-closed.", inst.name));
        }
    }
    if !exited {
        for p in &pids {
            let _ = proc::kill_tree(*p).await;
        }
        tokio::time::sleep(Duration::from_millis(600)).await;
    }
    state.rcon.disconnect(id).await;
    state.history.close_all_sessions(id);
    state
        .with_runtime(id, |r| {
            r.pid = None;
            r.state = Some(ServerState::Stopped);
            r.started_at = None;
            r.ready_at = None;
            r.players.clear();
            r.rcon_ok = false;
            r.pending_restart = false;
            r.restart_pending_at = None;
        })
        .await;
    manager_log(app, id, if exited { "Server stopped cleanly." } else { "Server process terminated." });
    if apply_pending(state, &inst).await.unwrap_or(false) {
        manager_log(app, id, "Applied config changes saved while the server was running.");
    }
    state.emit_status(app, id).await;

    if inst.automation.backup_on_stop && inst.db_path().exists() {
        state.set_busy(app, id, Some("Backing up")).await;
        let res = backup::create(state, &inst, "After stop", "auto").await;
        state.set_busy(app, id, None).await;
        match res {
            Ok(b) => {
                state.with_runtime(id, |r| r.last_backup = b.created_at).await;
                let _ = tauri::Emitter::emit(app, "backup-created", &b);
            }
            Err(e) => notify(app, id, "warn", format!("Backup after stop failed: {e}")),
        }
    }
    Ok(())
}

pub async fn restart(state: &AppState, app: &AppHandle, id: &str, update: bool) -> AppResult<()> {
    let lock = state.op_lock(id).await;
    let _g = lock.lock().await;
    let inst = state.instance(id).await?;
    state
        .with_runtime(id, |r| {
            r.state = Some(ServerState::Restarting);
            r.crash_loop = false;
        })
        .await;
    state.emit_status(app, id).await;
    stop_locked(state, app, id, false).await?;
    if update {
        state.with_runtime(id, |r| r.state = Some(ServerState::Updating)).await;
        state.emit_status(app, id).await;
        let res = update_files(state, app, &inst, false).await;
        state.with_runtime(id, |r| r.state = Some(ServerState::Stopped)).await;
        if let Err(e) = res {
            notify(app, id, "error", format!("Update failed, restarting on the current build: {e}"));
        }
    }
    start_locked(state, app, id).await
}

/// Restart with in-game countdown broadcasts. Skips the countdown when nobody is online.
/// `then` = "restart" | "update" | "stop". Cancellable via task id `countdown-<id>`.
pub async fn countdown(state: &AppState, app: &AppHandle, id: &str, then: &str, warn_minutes: &[u32], reason: &str) -> AppResult<()> {
    let inst = state.instance(id).await?;
    let mut marks: Vec<u32> = warn_minutes.iter().copied().filter(|m| *m > 0).collect();
    marks.sort_unstable_by(|a, b| b.cmp(a));
    marks.dedup();
    let players = state.with_runtime(id, |r| r.players.len()).await;
    let cancel = state.register_cancel(&format!("countdown-{id}")).await;
    if players > 0 && !marks.is_empty() {
        let total = marks[0];
        let at = now_ms() + total as i64 * 60_000;
        state.with_runtime(id, |r| r.restart_pending_at = Some(at)).await;
        state.emit_status(app, id).await;
        manager_log(app, id, format!("{} in {total} min ({reason}) – warning players.", capitalize(then)));
        for (i, m) in marks.iter().enumerate() {
            let msg = inst.automation.restart_message.replace("{minutes}", &m.to_string());
            let msg = if then == "stop" { msg.replace("restart", "shutdown") } else { msg };
            let _ = state.rcon_exec(&inst, &format!("broadcast {msg}")).await;
            let next = marks.get(i + 1).copied().unwrap_or(0);
            let wait_ms = (*m - next) as u64 * 60_000;
            let t0 = std::time::Instant::now();
            while (t0.elapsed().as_millis() as u64) < wait_ms {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    state.with_runtime(id, |r| r.restart_pending_at = None).await;
                    state.emit_status(app, id).await;
                    let _ = state.rcon_exec(&inst, "broadcast The scheduled restart was cancelled.").await;
                    manager_log(app, id, "Countdown cancelled.");
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
        let _ = state.rcon_exec(&inst, "broadcast Server is going down now.").await;
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    state.cancels.lock().await.remove(&format!("countdown-{id}"));
    state.with_runtime(id, |r| r.restart_pending_at = None).await;
    match then {
        "stop" => stop(state, app, id, false).await,
        "update" => restart(state, app, id, true).await,
        _ => restart(state, app, id, false).await,
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// SteamCMD install/update of one server's files (caller decides about stopping first).
pub async fn update_files(state: &AppState, app: &AppHandle, inst: &ServerInstance, validate: bool) -> AppResult<()> {
    let title = if !inst.is_installed() {
        format!("Installing {}", inst.name)
    } else if validate {
        format!("Validating {}", inst.name)
    } else {
        format!("Updating {}", inst.name)
    };
    let r = TaskReporter::new(app, "steamcmd", &title, Some(&inst.id));
    let cancel = state.register_cancel(&r.task_id).await;
    let steam_dir = state.steamcmd_dir().await;
    let res = {
        let _steam = state.steam_lock.lock().await;
        steamcmd::app_update(&state.http, &steam_dir, &inst.install_dir, &inst.branch, &inst.branch_password, validate, &r, cancel).await
    };
    state.cancels.lock().await.remove(&r.task_id);
    match res {
        Ok(()) => {
            let build = steamcmd::local_build(&inst.install_dir);
            if let Some(b) = &build {
                state.history.build(&inst.id, b, &inst.branch);
            }
            state.with_runtime(&inst.id, |rt| rt.installed_build = build.clone()).await;
            // Make sure the config folder exists so settings can be edited before the first start.
            let _ = sync_ini(inst);
            state.history.event(Some(&inst.id), "success", "update", &format!("{title} done – build {}", build.clone().unwrap_or_default()));
            r.done(&format!("{} – build {}", title.replace("ing ", "ed "), build.unwrap_or_else(|| "?".into())));
            Ok(())
        }
        Err(e) => {
            r.fail(&e.to_string());
            Err(e)
        }
    }
}

/// Full update job: stops the server if needed (with a backup), updates, restarts if it was up.
pub async fn update_job(state: &AppState, app: &AppHandle, id: &str, validate: bool) -> AppResult<()> {
    let lock = state.op_lock(id).await;
    let _g = lock.lock().await;
    let inst = state.instance(id).await?;
    let (_, main) = running_pids(&inst);
    let was_running = main.is_some();
    if was_running {
        manager_log(app, id, "Stopping the server to update it…");
        stop_locked(state, app, id, false).await?;
    }
    // stop_locked already backs up when "backup on stop" is on; otherwise honour "before update".
    if inst.automation.backup_before_update && !(was_running && inst.automation.backup_on_stop) && inst.db_path().exists() {
        if let Ok(b) = backup::create(state, &inst, "Before update", "auto").await {
            state.with_runtime(id, |r| r.last_backup = b.created_at).await;
        }
    }
    state.with_runtime(id, |r| r.state = Some(if inst.is_installed() { ServerState::Updating } else { ServerState::Installing })).await;
    state.emit_status(app, id).await;
    let res = update_files(state, app, &inst, validate).await;
    state.with_runtime(id, |r| r.state = None).await;
    state.emit_status(app, id).await;
    res?;
    if was_running {
        start_locked(state, app, id).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_keep_secrets_off_the_command_line() {
        let mut inst = ServerInstance::default();
        inst.identity.admin_password = "hunter2".into();
        inst.identity.server_password = "letmein".into();
        inst.rcon.password = "rconpw".into();
        inst.launch.multihome = "10.0.0.5".into();
        inst.launch.extra_args = "-Port=1 -foo \"-bar=a b\"".into();
        let a = build_args(&inst).join(" ");
        assert!(a.starts_with("-Port=7777 -QueryPort=27015 -MaxPlayers=40 -MULTIHOME=10.0.0.5 -MULTIHOMEHTTP=10.0.0.5"), "{a}");
        assert!(a.contains("-log") && a.contains("-foo") && a.contains("\"-bar=a b\""));
        assert!(!a.contains("-Port=1 "));
        assert!(!a.contains("hunter2") && !a.contains("letmein") && !a.contains("rconpw"));
    }

    #[test]
    fn sync_preserves_server_owned_lines() {
        let dir = std::env::temp_dir().join(format!("cesm-sync-{}", uuid::Uuid::new_v4()));
        let mut inst = ServerInstance { install_dir: dir.clone(), ..Default::default() };
        inst.identity.server_name = "My Server".into();
        inst.identity.admin_password = "pw".into();
        inst.rcon.password = "rc".into();
        inst.regions.excluded = vec!["IsleOfSiptah".into(), "Bogus".into()];
        inst.mods = vec![ModEntry { workshop_id: "123".into(), ..Default::default() }];
        std::fs::create_dir_all(inst.config_dir()).unwrap();
        // Real first lines of a server-written ServerSettings.ini.
        std::fs::write(inst.settings_path(), ";METADATA=(Diff=true, UseCommands=true)\r\n[ServerSettings]\r\nConfigVersion=13\r\nBlueprintConfigVersion=25\r\nHarvestAmountMultiplier=2\r\n").unwrap();
        std::fs::write(inst.engine_ini(), "[OnlineSubsystem]\r\nbUseBuildIdOverride=True\r\nBuildIdOverride=460304166\r\n").unwrap();
        let notes = sync_ini(&inst).unwrap();
        assert_eq!(notes.len(), 1);
        let s = std::fs::read_to_string(inst.settings_path()).unwrap();
        assert!(s.starts_with(";METADATA=(Diff=true, UseCommands=true)\r\n[ServerSettings]\r\nConfigVersion=13\r\nBlueprintConfigVersion=25\r\nHarvestAmountMultiplier=2\r\n"));
        assert!(s.contains("AdminPassword=pw") && s.contains("ServerModList=123"));
        let e = std::fs::read_to_string(inst.engine_ini()).unwrap();
        assert!(e.contains("ServerName=My Server") && !e.contains("BuildIdOverride"));
        assert!(e.contains("[Regions]\r\nExcludedRegions=IsleOfSiptah\r\n"));
        let g = std::fs::read_to_string(inst.game_ini()).unwrap();
        assert!(g.contains("RconEnabled=1") && g.contains("RconPassword=rc") && g.contains("RconPort=25575"));

        let mut back = ServerInstance { install_dir: dir.clone(), ..Default::default() };
        back.rcon.password.clear();
        read_ini_into(&mut back);
        assert_eq!(back.identity.server_name, "My Server");
        assert_eq!(back.identity.admin_password, "pw");
        assert_eq!(back.rcon.password, "rc");
        assert_eq!(back.regions.excluded, vec!["IsleOfSiptah"]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
