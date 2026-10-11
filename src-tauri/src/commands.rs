//! Every `#[tauri::command]`. Thin wrappers that delegate to `core::*`; mutating commands write
//! to the audit log. Add new commands to `lib.rs`, `src/lib/ipc.ts` and `src/lib/devMock.ts`.

use crate::core::{backup, diagnostics, gamedb, history, ini, maps, mods, modsync, network, proc, rcon, secrets, server, settings, steamcmd, supervisor, workshop};
use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::state::{manager_log, notify, now_ms, AppState, Folder};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

type St<'a> = State<'a, Arc<AppState>>;

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> AppResult<T> + Send + 'static) -> AppResult<T> {
    tokio::task::spawn_blocking(f).await.map_err(|e| AppError::msg(e.to_string()))?
}

/// Run a long job in the background; errors become toasts. Returns immediately.
fn background<F>(app: &AppHandle, id: &str, what: &'static str, fut: F)
where
    F: std::future::Future<Output = AppResult<()>> + Send + 'static,
{
    let (app2, id) = (app.clone(), id.to_string());
    tauri::async_runtime::spawn(async move {
        if let Err(e) = fut.await {
            notify(&app2, &id, "error", format!("{what} failed: {e}"));
        }
    });
}

fn require_stopped(inst: &ServerInstance) -> AppResult<()> {
    if server::running_pids(inst).1.is_some() {
        return Err(AppError::msg("Stop the server first – it uses these files while running."));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------- app

#[tauri::command]
pub async fn get_app_info(state: St<'_>) -> AppResult<AppInfo> {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    sys.refresh_cpu_list(sysinfo::CpuRefreshKind::nothing());
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: state.data_dir.clone(),
        portable: state.portable,
        server_app_id: SERVER_APP_ID,
        game_app_id: GAME_APP_ID,
        cores: sys.cpus().len(),
        total_memory: sys.total_memory(),
        os: sysinfo::System::long_os_version().unwrap_or_else(|| "Windows".into()),
        hostname: sysinfo::System::host_name().unwrap_or_default(),
    })
}

#[tauri::command]
pub async fn get_settings(state: St<'_>) -> AppResult<AppSettings> {
    Ok(state.settings.read().await.clone())
}

#[tauri::command]
pub async fn save_settings(state: St<'_>, settings: AppSettings) -> AppResult<()> {
    let mut s = settings;
    s.last_data_dir = Some(state.data_dir.clone());
    *state.settings.write().await = s;
    state.history.audit(None, "save-settings", "");
    state.persist_settings().await
}

#[tauri::command]
pub async fn test_webhook(state: St<'_>, url: String) -> AppResult<()> {
    crate::state::send_webhook(&state.http, url.trim(), "success", "", "Test message from Conan Exiles Enhanced Server Manager \u{2013} notifications work!").await
}

#[tauri::command]
pub fn app_quit(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn window_hide(window: tauri::WebviewWindow) {
    let _ = window.hide();
}

#[tauri::command]
pub async fn cancel_task(state: St<'_>, task_id: String) -> AppResult<()> {
    if let Some(f) = state.cancels.lock().await.get(&task_id) {
        f.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderProbe {
    exists: bool,
    writable: bool,
    empty: bool,
    has_server: bool,
    free_bytes: Option<u64>,
}

#[tauri::command]
pub async fn probe_folder(path: String) -> AppResult<FolderProbe> {
    blocking(move || {
        let p = PathBuf::from(&path);
        let exists = p.exists();
        let empty = !exists || std::fs::read_dir(&p).map(|mut r| r.next().is_none()).unwrap_or(true);
        let has_server = resolve_server_exe(&p).exists();
        let probe_dir = if exists { p.clone() } else { p.ancestors().find(|a| a.exists()).map(|a| a.to_path_buf()).unwrap_or(p.clone()) };
        let t = probe_dir.join(format!(".cesm-{}", uuid::Uuid::new_v4()));
        let writable = std::fs::write(&t, b"").is_ok();
        let _ = std::fs::remove_file(&t);
        let disks = sysinfo::Disks::new_with_refreshed_list();
        let n = proc::norm(&probe_dir);
        let free_bytes = disks
            .iter()
            .filter(|d| n.starts_with(&proc::norm(d.mount_point())))
            .max_by_key(|d| d.mount_point().as_os_str().len())
            .map(|d| d.available_space());
        Ok(FolderProbe { exists, writable, empty, has_server, free_bytes })
    })
    .await
}

#[tauri::command]
pub async fn list_folders(state: St<'_>) -> AppResult<Vec<Folder>> {
    Ok(state.folders.read().await.clone())
}

#[tauri::command]
pub async fn save_folders(state: St<'_>, folders: Vec<Folder>) -> AppResult<()> {
    *state.folders.write().await = folders;
    state.persist_folders().await
}

#[tauri::command]
pub async fn audit_log(state: St<'_>, limit: u32) -> AppResult<Vec<history::AuditRow>> {
    Ok(state.history.audit_log(limit.clamp(1, 5000)))
}

#[tauri::command]
pub async fn event_log(state: St<'_>, id: Option<String>, limit: u32) -> AppResult<Vec<history::EventRow>> {
    Ok(state.history.events(id.as_deref(), limit.clamp(1, 5000)))
}

#[tauri::command]
pub async fn metrics_history(state: St<'_>, id: String, since: i64) -> AppResult<Vec<history::MetricRow>> {
    Ok(state.history.metrics(&id, since))
}

#[tauri::command]
pub async fn build_history(state: St<'_>, id: String) -> AppResult<Vec<(String, String, i64)>> {
    Ok(state.history.builds(&id))
}

// ---------------------------------------------------------------------------------- instances

#[tauri::command]
pub async fn list_instances(state: St<'_>) -> AppResult<Vec<ServerInstance>> {
    Ok(state.instances.read().await.clone())
}

#[tauri::command]
pub async fn get_statuses(state: St<'_>) -> AppResult<Vec<ServerStatus>> {
    let list = state.instances.read().await.clone();
    let mut out = vec![];
    for i in &list {
        out.push(state.status(i).await);
    }
    Ok(out)
}

#[tauri::command]
pub async fn suggest_ports(state: St<'_>, exclude_id: Option<String>) -> AppResult<network::PortSet> {
    let list = state.instances.read().await.clone();
    Ok(network::suggest_ports(&list, exclude_id.as_deref()).await)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateInput {
    name: String,
    install_dir: Option<PathBuf>,
    color: Option<String>,
    identity: Identity,
    ini: Option<DedicatedSettings>,
    port: Option<u16>,
    query_port: Option<u16>,
    rcon_port: Option<u16>,
    max_players: Option<u32>,
    branch: Option<String>,
    install_now: bool,
    /// Copy the profile (settings, automation, mods) of an existing server.
    template_id: Option<String>,
    /// Initial game rules (from a preset) – written into ServerSettings.ini.
    #[serde(default)]
    rules: BTreeMap<String, String>,
}

/// Folders must be absolute – a relative path would resolve against wherever the app (or
/// SteamCMD, which then silently falls back to its own library) happens to run.
fn abs_dir(p: PathBuf) -> AppResult<PathBuf> {
    let s = p.to_string_lossy().trim().to_string();
    let p = PathBuf::from(&s);
    if !p.is_absolute() {
        return Err(AppError::msg(format!("“{s}” isn't a full folder path – use something like D:\\ConanServers\\MyServer.")));
    }
    Ok(p)
}

fn slug(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() {
        "server".into()
    } else {
        s.chars().take(40).collect()
    }
}

async fn unique_dir(state: &AppState, name: &str) -> PathBuf {
    let root = state.default_install_root().await;
    let base = slug(name);
    let mut p = root.join(&base);
    let mut n = 2;
    let used: Vec<String> = state.instances.read().await.iter().map(|i| proc::norm(&i.install_dir)).collect();
    while p.exists() && std::fs::read_dir(&p).map(|mut r| r.next().is_some()).unwrap_or(false) || used.contains(&proc::norm(&p)) {
        p = root.join(format!("{base}-{n}"));
        n += 1;
    }
    p
}

async fn add_instance(state: &AppState, inst: ServerInstance) -> AppResult<ServerInstance> {
    {
        let mut list = state.instances.write().await;
        if list.iter().any(|i| proc::norm(&i.install_dir) == proc::norm(&inst.install_dir)) {
            return Err(AppError::msg("Another server profile already uses that folder."));
        }
        list.push(inst.clone());
    }
    state.persist_instances().await?;
    state.history.audit(Some(&inst.id), "create", &inst.install_dir.display().to_string());
    Ok(inst)
}

#[tauri::command]
pub async fn create_instance(app: AppHandle, state: St<'_>, input: CreateInput) -> AppResult<ServerInstance> {
    let name = if input.name.trim().is_empty() { "Conan Exiles Server".to_string() } else { input.name.trim().to_string() };
    let mut inst = match &input.template_id {
        Some(t) => state.instance(t).await?,
        None => ServerInstance::default(),
    };
    inst.id = uuid::Uuid::new_v4().to_string();
    inst.name = name.clone();
    inst.created_at = now_ms();
    inst.favorite = false;
    inst.archived = false;
    inst.color = input.color.unwrap_or_else(|| "bronze".into());
    inst.install_dir = match input.install_dir {
        Some(d) if !d.as_os_str().is_empty() => abs_dir(d)?,
        _ => unique_dir(&state, &name).await,
    };
    inst.identity = input.identity;
    if let Some(i) = input.ini {
        inst.ini = i;
    }
    if inst.identity.server_name.trim().is_empty() {
        inst.identity.server_name = name.clone();
    }
    if let Some(b) = input.branch.filter(|b| !b.trim().is_empty()) {
        inst.branch = b;
    }
    let list = state.instances.read().await.clone();
    let free = network::suggest_ports(&list, None).await;
    inst.launch.port = input.port.filter(|p| *p > 0).unwrap_or(free.game);
    inst.launch.query_port = input.query_port.filter(|p| *p > 0).unwrap_or(free.query);
    inst.rcon.port = input.rcon_port.filter(|p| *p > 0).unwrap_or(free.rcon);
    inst.rcon.enabled = true;
    inst.rcon.password = secrets::random_password(24);
    if let Some(m) = input.max_players {
        inst.launch.max_players = m.clamp(1, 200);
    }
    std::fs::create_dir_all(&inst.install_dir)?;
    // An install that already exists in the chosen folder keeps its world, config and mod list.
    if inst.is_installed() {
        let mut probe = inst.clone();
        server::read_ini_into(&mut probe);
        if inst.identity.admin_password.is_empty() {
            inst.identity.admin_password = probe.identity.admin_password;
        }
        if inst.mods.is_empty() {
            inst.mods = probe.mods;
        }
    }
    let _ = server::sync_ini(&inst);
    if !input.rules.is_empty() {
        settings::write_file_values(&inst, &input.rules)?;
    }
    let inst = add_instance(&state, inst).await?;
    if input.install_now {
        let (st, app2, id) = (state.inner().clone(), app.clone(), inst.id.clone());
        background(&app, &inst.id, "Install", async move { server::update_job(&st, &app2, &id, false).await });
    }
    state.emit_status(&app, &inst.id).await;
    Ok(inst)
}

#[tauri::command]
pub async fn update_instance(app: AppHandle, state: St<'_>, instance: ServerInstance) -> AppResult<ServerInstance> {
    let old = state.instance(&instance.id).await?;
    let mut inst = instance;
    inst.install_dir = abs_dir(inst.install_dir)?;
    inst.launch.max_players = inst.launch.max_players.clamp(1, 200);
    if inst.launch.port == 0 || inst.launch.query_port == 0 || inst.rcon.port == 0 {
        return Err(AppError::msg("Ports can't be 0."));
    }
    if inst.launch.query_port == inst.launch.port || inst.launch.query_port == inst.pinger_port() {
        return Err(AppError::msg(format!("The query port must differ from the game port and game port + 1 ({}).", inst.pinger_port())));
    }
    if inst.rcon.enabled && inst.rcon.password.is_empty() {
        inst.rcon.password = secrets::random_password(24);
    }
    let (_, main) = server::running_pids(&old);
    if proc::norm(&old.install_dir) != proc::norm(&inst.install_dir) && main.is_some() {
        return Err(AppError::msg("Stop the server before changing its folder."));
    }
    if main.is_some() && (old.rcon.enabled != inst.rcon.enabled || old.rcon.password != inst.rcon.password || old.rcon.port != inst.rcon.port) {
        // The running server keeps the credentials it started with; changing them now would cut
        // the manager off (no clean stop, no live settings).
        return Err(AppError::msg("Stop the server before changing RCON port or password."));
    }
    state.put_instance(&inst).await?;
    state.history.audit(Some(&inst.id), "update-profile", "");
    let build = steamcmd::local_build(&inst.install_dir);
    state.with_runtime(&inst.id, |r| r.installed_build = build).await;
    let restart_needed = old.identity != inst.identity
        || old.launch.port != inst.launch.port
        || old.launch.query_port != inst.launch.query_port
        || old.launch.max_players != inst.launch.max_players
        || old.launch.multihome != inst.launch.multihome
        || old.launch.multihome_http != inst.launch.multihome_http
        || old.launch.options != inst.launch.options
        || old.launch.extra_args != inst.launch.extra_args
        || old.rcon != inst.rcon
        || old.regions != inst.regions
        || old.mods != inst.mods;
    match main {
        Some(pid) => {
            if old.launch.priority != inst.launch.priority || old.launch.affinity != inst.launch.affinity {
                let _ = proc::apply_priority_affinity(pid, &inst.launch.priority, &inst.launch.affinity);
            }
            if old.launch.show_console != inst.launch.show_console {
                let (pids, _) = server::running_pids(&inst);
                proc::set_console_visible(&pids, inst.launch.show_console);
                state.with_runtime(&inst.id, |r| r.console_hidden = !inst.launch.show_console).await;
            }
            // The MOTD can be changed live.
            if old.identity.motd != inst.identity.motd {
                let _ = state.rcon_exec(&old, &format!("SetServerSetting ServerMessageOfTheDay {}", inst.identity.motd)).await;
            }
            if restart_needed {
                state.with_runtime(&inst.id, |r| r.pending_restart = true).await;
            }
        }
        None => {
            if inst.config_dir().exists() || inst.is_installed() {
                let i2 = inst.clone();
                blocking(move || server::sync_ini(&i2).map(|_| ())).await?;
            }
            state.rcon.disconnect(&inst.id).await;
        }
    }
    state.emit_status(&app, &inst.id).await;
    let _ = app.emit("instances-changed", ());
    Ok(inst)
}

#[tauri::command]
pub async fn reorder_instances(state: St<'_>, ids: Vec<String>) -> AppResult<()> {
    {
        let mut list = state.instances.write().await;
        list.sort_by_key(|i| ids.iter().position(|x| x == &i.id).unwrap_or(usize::MAX));
    }
    state.persist_instances().await
}

#[tauri::command]
pub async fn delete_instance(_app: AppHandle, state: St<'_>, id: String, delete_files: bool) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    if let (_, Some(_)) = server::running_pids(&inst) {
        // Force-kill directly: no graceful shutdown, no backup, no apply_pending — we're deleting.
        let (pids, _) = server::running_pids(&inst);
        for p in &pids {
            let _ = proc::kill_tree(*p).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        state.rcon.disconnect(&id).await;
        state.with_runtime(&id, |r| { r.pid = None; r.state = Some(ServerState::Stopped); }).await;
    }
    state.instances.write().await.retain(|i| i.id != id);
    state.persist_instances().await?;
    state.runtime.lock().await.remove(&id);
    state.rcon.disconnect(&id).await;
    state.history.audit(Some(&id), "delete", &format!("{} delete_files={delete_files}", inst.install_dir.display()));
    if delete_files && inst.install_dir.exists() {
        // Keep backups: they live inside the folder by default, so move them out first.
        let dir = inst.install_dir.clone();
        let keep = state.data_dir.join("deleted-server-backups").join(&id);
        blocking(move || {
            let bk = dir.join("Backups");
            if bk.exists() {
                std::fs::create_dir_all(keep.parent().unwrap())?;
                let _ = std::fs::rename(&bk, &keep);
            }
            std::fs::remove_dir_all(&dir)?;
            Ok(())
        })
        .await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn clone_instance(state: St<'_>, id: String, name: String, copy_saves: bool) -> AppResult<ServerInstance> {
    let src = state.instance(&id).await?;
    let mut inst = src.clone();
    inst.id = uuid::Uuid::new_v4().to_string();
    inst.name = name.trim().to_string();
    inst.identity.server_name = inst.name.clone();
    inst.created_at = now_ms();
    inst.install_dir = unique_dir(&state, &inst.name).await;
    let list = state.instances.read().await.clone();
    let free = network::suggest_ports(&list, None).await;
    inst.launch.port = free.game;
    inst.launch.query_port = free.query;
    inst.rcon.port = free.rcon;
    inst.rcon.password = secrets::random_password(24);
    let (s2, d2) = (src.clone(), inst.clone());
    let tmp = state.data_dir.join("tmp");
    blocking(move || {
        std::fs::create_dir_all(d2.config_dir())?;
        for f in ["ServerSettings.ini", "DedicatedServer.ini", "Engine.ini", "Game.ini", "ModHttp.ini"] {
            let p = s2.config_dir().join(f);
            if p.exists() {
                ini::write_file(&d2.config_dir().join(f), &ini::read_file(&p)?)?;
            }
        }
        if copy_saves && s2.db_path().exists() {
            std::fs::create_dir_all(d2.saved_dir())?;
            std::fs::create_dir_all(&tmp)?;
            gamedb::snapshot(&s2.db_path(), &d2.saved_dir().join("game_0.db"))?;
        }
        server::sync_ini(&d2).map(|_| ())
    })
    .await?;
    add_instance(&state, inst).await
}

#[tauri::command]
pub async fn find_existing_installs(state: St<'_>) -> AppResult<Vec<PathBuf>> {
    let known: Vec<String> = state.instances.read().await.iter().map(|i| proc::norm(&i.install_dir)).collect();
    Ok(steamcmd::find_steam_client_installs().await.into_iter().filter(|p| !known.contains(&proc::norm(p))).collect())
}

#[tauri::command]
pub async fn import_existing(app: AppHandle, state: St<'_>, path: PathBuf, name: Option<String>) -> AppResult<ServerInstance> {
    let path = abs_dir(path)?;
    if !resolve_server_exe(&path).exists() {
        return Err(AppError::msg("No Conan Exiles dedicated server found in that folder (looked for ConanSandbox\\Binaries\\Win64\\ConanSandboxServer-Win64-Shipping.exe)."));
    }
    let mut inst = ServerInstance { id: uuid::Uuid::new_v4().to_string(), created_at: now_ms(), install_dir: path.clone(), ..Default::default() };
    server::read_ini_into(&mut inst);
    // Detect the branch from the Steam manifest.
    if let Ok(acf) = std::fs::read_to_string(path.join("steamapps").join(format!("appmanifest_{SERVER_APP_ID}.acf"))) {
        if let Some(c) = regex::Regex::new(r#""BetaKey"\s+"([^"]+)""#).ok().and_then(|r| r.captures(&acf).map(|c| c[1].to_string())) {
            inst.branch = c;
        }
    }
    inst.name = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| inst.identity.server_name.clone());
    let list = state.instances.read().await.clone();
    if list.iter().any(|i| i.udp_ports().iter().any(|p| inst.udp_ports().contains(p)) || i.rcon.port == inst.rcon.port) {
        let free = network::suggest_ports(&list, None).await;
        inst.launch.port = free.game;
        inst.launch.query_port = free.query;
        inst.rcon.port = free.rcon;
    }
    let inst = add_instance(&state, inst).await?;
    let build = steamcmd::local_build(&inst.install_dir);
    state.with_runtime(&inst.id, |r| r.installed_build = build).await;
    state.emit_status(&app, &inst.id).await;
    Ok(inst)
}

// ---------------------------------------------------------------------------------- lifecycle

#[tauri::command]
pub async fn install_server(app: AppHandle, state: St<'_>, id: String, validate: bool) -> AppResult<()> {
    let st = state.inner().clone();
    let (app2, id2) = (app.clone(), id.clone());
    state.history.audit(Some(&id), if validate { "validate" } else { "update" }, "");
    background(&app, &id, if validate { "Validate" } else { "Update" }, async move { server::update_job(&st, &app2, &id2, validate).await });
    Ok(())
}

#[tauri::command]
pub async fn check_update(app: AppHandle, state: St<'_>, id: String) -> AppResult<bool> {
    let st = state.inner().clone();
    state.latest_build_cache.lock().await.clear();
    supervisor::check_update(&st, &app, &id, false).await
}

#[tauri::command]
pub async fn start_server(app: AppHandle, state: St<'_>, id: String) -> AppResult<()> {
    state.with_runtime(&id, |r| r.crash_loop = false).await;
    let st = state.inner().clone();
    let (app2, id2) = (app.clone(), id.clone());
    // Mod downloads can take a while – return at once, status events follow.
    background(&app, &id, "Start", async move { server::start(&st, &app2, &id2).await });
    Ok(())
}

#[tauri::command]
pub async fn stop_server(app: AppHandle, state: St<'_>, id: String, force: bool, warn: bool) -> AppResult<()> {
    let st = state.inner().clone();
    let (app2, id2) = (app.clone(), id.clone());
    let inst = state.instance(&id).await?;
    let warn_minutes = if warn && !force { inst.automation.restart_warnings.clone() } else { vec![] };
    background(&app, &id, "Stop", async move {
        if warn_minutes.is_empty() {
            server::stop(&st, &app2, &id2, force).await
        } else {
            server::countdown(&st, &app2, &id2, "stop", &warn_minutes, "manual").await
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn restart_server(app: AppHandle, state: St<'_>, id: String, update: bool, warn: bool) -> AppResult<()> {
    let st = state.inner().clone();
    let (app2, id2) = (app.clone(), id.clone());
    let inst = state.instance(&id).await?;
    let warn_minutes = if warn { inst.automation.restart_warnings.clone() } else { vec![] };
    background(&app, &id, "Restart", async move {
        if warn_minutes.is_empty() {
            server::restart(&st, &app2, &id2, update).await
        } else {
            server::countdown(&st, &app2, &id2, if update { "update" } else { "restart" }, &warn_minutes, "manual").await
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn cancel_countdown(state: St<'_>, id: String) -> AppResult<()> {
    if let Some(f) = state.cancels.lock().await.get(&format!("countdown-{id}")) {
        f.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub async fn get_launch_line(state: St<'_>, id: String) -> AppResult<String> {
    Ok(server::launch_line(&state.instance(&id).await?))
}

#[tauri::command]
pub async fn preview_launch_line(instance: ServerInstance) -> AppResult<String> {
    Ok(server::launch_line(&instance))
}

#[tauri::command]
pub async fn set_console_visible(state: St<'_>, id: String, visible: bool) -> AppResult<usize> {
    let inst = state.instance(&id).await?;
    let (pids, _) = server::running_pids(&inst);
    state.with_runtime(&id, |r| r.console_hidden = !visible).await;
    Ok(proc::set_console_visible(&pids, visible))
}

#[tauri::command]
pub async fn clear_crash_state(app: AppHandle, state: St<'_>, id: String) -> AppResult<()> {
    state
        .with_runtime(&id, |r| {
            r.crash_loop = false;
            r.crash_times.clear();
            r.restart_at = None;
            if r.state == Some(ServerState::Crashed) {
                r.state = Some(ServerState::Stopped);
            }
        })
        .await;
    state.emit_status(&app, &id).await;
    Ok(())
}

// --------------------------------------------------------------------------------- game rules

#[tauri::command]
pub async fn read_game_rules(state: St<'_>, id: String) -> AppResult<settings::GameRules> {
    let inst = state.instance(&id).await?;
    let pending = state.with_runtime(&id, |r| r.pending_settings.clone()).await;
    let mut r = blocking(move || settings::read(&inst)).await?;
    r.pending = pending;
    Ok(r)
}

#[tauri::command]
pub async fn apply_game_rules(
    app: AppHandle,
    state: St<'_>,
    id: String,
    changes: BTreeMap<String, String>,
    live_values: BTreeMap<String, String>,
    restart_keys: Vec<String>,
) -> AppResult<settings::ApplyResult> {
    let inst = state.instance(&id).await?;
    let detail = changes.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(", ");
    let res = settings::apply(&state, &inst, changes, &live_values, &restart_keys).await?;
    state.history.audit(Some(&id), "game-rules", &detail);
    if !res.live.is_empty() {
        manager_log(&app, &id, format!("Applied live: {}", res.live.join(", ")));
    }
    state.emit_status(&app, &id).await;
    Ok(res)
}

#[tauri::command]
pub async fn reset_game_rules(state: St<'_>, id: String, keys: Vec<String>) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    require_stopped(&inst)?;
    state.history.audit(Some(&id), "game-rules-reset", &keys.join(", "));
    blocking(move || settings::remove_file_values(&inst, &keys)).await
}

// ------------------------------------------------------------------------------------- config

const CONFIG_FILES: [&str; 5] = ["ServerSettings.ini", "DedicatedServer.ini", "Engine.ini", "Game.ini", "ModHttp.ini"];

fn check_file(file: &str) -> AppResult<()> {
    if CONFIG_FILES.contains(&file) {
        Ok(())
    } else {
        Err(AppError::msg("Unknown config file"))
    }
}

#[tauri::command]
pub async fn read_config(state: St<'_>, id: String, file: String) -> AppResult<ConfigFile> {
    check_file(&file)?;
    let inst = state.instance(&id).await?;
    let pending = state.with_runtime(&id, |r| r.pending_configs.get(&file).cloned()).await;
    let path = inst.config_dir().join(&file);
    blocking(move || {
        let meta = std::fs::metadata(&path).ok();
        Ok(ConfigFile {
            exists: meta.is_some(),
            modified: meta.and_then(|m| m.modified().ok()).map(|t| chrono::DateTime::<chrono::Utc>::from(t).timestamp_millis()),
            text: match pending {
                Some(t) => t,
                None => ini::read_file(&path)?,
            },
            path,
        })
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteResult {
    deferred: bool,
}

#[tauri::command]
pub async fn write_config(app: AppHandle, state: St<'_>, id: String, file: String, text: String) -> AppResult<WriteResult> {
    check_file(&file)?;
    let inst = state.instance(&id).await?;
    let deferred = server::running_pids(&inst).1.is_some();
    state.history.audit(Some(&id), "write-config", &file);
    if deferred {
        state
            .with_runtime(&id, |r| {
                r.pending_configs.insert(file.clone(), text.clone());
                r.pending_restart = true;
            })
            .await;
    } else {
        let p = inst.config_dir().join(&file);
        blocking(move || ini::write_file(&p, &text)).await?;
    }
    state.emit_status(&app, &id).await;
    Ok(WriteResult { deferred })
}

#[tauri::command]
pub async fn list_config_history(state: St<'_>, id: String, file: String) -> AppResult<Vec<(String, i64)>> {
    check_file(&file)?;
    let inst = state.instance(&id).await?;
    let dir = inst.config_dir().join("_history");
    blocking(move || {
        let mut v: Vec<(String, i64)> = std::fs::read_dir(&dir)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.file_name().to_string_lossy().starts_with(&format!("{file}.")))
                    .map(|e| {
                        let t = e.metadata().ok().and_then(|m| m.modified().ok()).map(|t| chrono::DateTime::<chrono::Utc>::from(t).timestamp_millis()).unwrap_or(0);
                        (e.path().to_string_lossy().into_owned(), t)
                    })
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        Ok(v)
    })
    .await
}

#[tauri::command]
pub async fn read_text_file(state: St<'_>, id: String, path: PathBuf) -> AppResult<String> {
    let inst = state.instance(&id).await?;
    if !proc::is_within(&path, &inst.install_dir) {
        return Err(AppError::msg("That file is outside the server folder."));
    }
    blocking(move || ini::read_file(&path)).await
}

// ---------------------------------------------------------------------------------------- logs

#[tauri::command]
pub async fn read_log_tail(state: St<'_>, id: String, max_lines: usize, path: Option<PathBuf>) -> AppResult<Vec<String>> {
    let inst = state.instance(&id).await?;
    let path = match path {
        Some(p) if proc::is_within(&p, &inst.logs_dir()) => p,
        Some(_) => return Err(AppError::msg("That file isn't one of this server's logs.")),
        None => inst.log_path(),
    };
    blocking(move || {
        let Ok(meta) = std::fs::metadata(&path) else { return Ok(vec![]) };
        use std::io::{Read, Seek, SeekFrom};
        let len = meta.len();
        let start = len.saturating_sub(8 * 1_048_576);
        let mut f = std::fs::File::open(&path)?;
        f.seek(SeekFrom::Start(start))?;
        let mut buf = vec![];
        f.read_to_end(&mut buf)?;
        let text = String::from_utf8_lossy(&buf);
        let lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
        let skip = lines.len().saturating_sub(max_lines.clamp(10, 50_000));
        Ok(lines[skip..].to_vec())
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFile {
    path: PathBuf,
    name: String,
    size: u64,
    modified: i64,
}

#[tauri::command]
pub async fn list_log_files(state: St<'_>, id: String) -> AppResult<Vec<LogFile>> {
    let inst = state.instance(&id).await?;
    blocking(move || {
        let mut v: Vec<LogFile> = std::fs::read_dir(inst.logs_dir())
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.path().extension().map(|x| x == "log").unwrap_or(false))
                    .filter_map(|e| {
                        let m = e.metadata().ok()?;
                        Some(LogFile {
                            name: e.file_name().to_string_lossy().into_owned(),
                            path: e.path(),
                            size: m.len(),
                            modified: m.modified().ok().map(|t| chrono::DateTime::<chrono::Utc>::from(t).timestamp_millis()).unwrap_or(0),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by(|a, b| b.modified.cmp(&a.modified));
        Ok(v)
    })
    .await
}

// ---------------------------------------------------------------------------------------- rcon

#[tauri::command]
pub async fn rcon_command(app: AppHandle, state: St<'_>, id: String, command: String) -> AppResult<String> {
    let inst = state.instance(&id).await?;
    let cmd = command.trim().to_string();
    if cmd.eq_ignore_ascii_case("shutdown") || cmd.eq_ignore_ascii_case("restart") {
        return Err(AppError::msg("Use the Stop / Restart buttons – they save, back up and track the server properly."));
    }
    state.history.audit(Some(&id), "rcon", &cmd);
    let out = state.rcon_exec(&inst, &cmd).await?;
    let _ = app.emit("log-lines", LogBatch { id: id.clone(), source: "rcon".into(), lines: vec![format!("> {cmd}"), out.clone()], ts: now_ms() });
    Ok(out)
}

// ------------------------------------------------------------------------------------- players

#[tauri::command]
pub async fn player_history(state: St<'_>, id: String) -> AppResult<Vec<history::PlayerSummary>> {
    Ok(state.history.players(&id))
}

#[tauri::command]
pub async fn server_stats(state: St<'_>, id: String) -> AppResult<Option<history::ServerStats>> {
    Ok(state.history.server_stats(&id))
}

#[tauri::command]
pub async fn game_characters(state: St<'_>, id: String) -> AppResult<Vec<gamedb::Character>> {
    let inst = state.instance(&id).await?;
    blocking(move || gamedb::characters(&inst.db_path())).await
}

#[tauri::command]
pub async fn game_clans(state: St<'_>, id: String) -> AppResult<Vec<gamedb::Clan>> {
    let inst = state.instance(&id).await?;
    blocking(move || gamedb::clans(&inst.db_path())).await
}

#[tauri::command]
pub async fn game_events(state: St<'_>, id: String, limit: u32, filter: String) -> AppResult<Vec<gamedb::GameEvent>> {
    let inst = state.instance(&id).await?;
    blocking(move || gamedb::events(&inst.db_path(), limit, &filter)).await
}

#[tauri::command]
pub async fn world_summary(state: St<'_>, id: String) -> AppResult<gamedb::WorldSummary> {
    let inst = state.instance(&id).await?;
    blocking(move || gamedb::summary(&inst.db_path())).await
}

/// Discover the maps this install can run by scanning its `Content` folder for `.umap` assets.
#[tauri::command]
pub async fn list_maps(state: St<'_>, id: String) -> AppResult<Vec<maps::MapInfo>> {
    let inst = state.instance(&id).await?;
    let content = inst.content_dir();
    blocking(move || Ok(maps::list_maps(&content))).await
}

/// Discover maps by scanning an install folder directly (create wizard – no instance exists yet).
/// Accepts the install dir (`…/ConanSandbox/Content`), the project dir, or the Content dir itself.
#[tauri::command]
pub async fn list_maps_in(install_dir: String) -> AppResult<Vec<maps::MapInfo>> {
    let root = PathBuf::from(install_dir);
    let candidates = [root.join(PROJECT).join("Content"), root.join("Content"), root.clone()];
    let content = candidates.iter().find(|p| p.is_dir()).cloned().unwrap_or_else(|| candidates[0].clone());
    blocking(move || Ok(maps::list_maps(&content))).await
}

#[tauri::command]
pub async fn sql_query(state: St<'_>, id: String, sql: String) -> AppResult<rcon::SqlTable> {
    let inst = state.instance(&id).await?;
    state.history.audit(Some(&id), "sql", &sql);
    blocking(move || gamedb::query(&inst.db_path(), &sql, 2000)).await
}

/// kick | ban | unban | whitelist | unwhitelist | warn (warn = in-game message + record only)
#[tauri::command]
pub async fn player_action(state: St<'_>, id: String, action: String, user_id: String, player_name: String, reason: String) -> AppResult<String> {
    let inst = state.instance(&id).await?;
    let target = if user_id.trim().is_empty() { player_name.trim().to_string() } else { user_id.trim().to_string() };
    if target.is_empty() {
        return Err(AppError::msg("No player selected."));
    }
    let reason1 = reason.replace(['\r', '\n'], " ");
    let cmd = match action.as_str() {
        "kick" => format!("kickplayer {} {} {}", if user_id.is_empty() { "name" } else { "userid" }, target, reason1),
        "ban" => format!("banplayer {} {} {}", if user_id.is_empty() { "name" } else { "userid" }, target, reason1),
        "unban" => format!("unbanplayer {target}"),
        "whitelist" => format!("whitelistplayer {target}"),
        "unwhitelist" => format!("unwhitelistplayer {target}"),
        "warn" => format!("broadcast {}: {}", player_name, if reason1.is_empty() { "please follow the server rules.".to_string() } else { reason1.clone() }),
        _ => return Err(AppError::msg("Unknown player action")),
    };
    let out = state.rcon_exec(&inst, cmd.trim()).await?;
    state.history.moderation(&id, &target, &player_name, &action, &reason);
    state.history.audit(Some(&id), &format!("player-{action}"), &format!("{target} {reason}"));
    Ok(out)
}

#[tauri::command]
pub async fn set_player_note(state: St<'_>, id: String, user_id: String, note: String) -> AppResult<()> {
    state.history.set_note(&id, &user_id, &note);
    Ok(())
}

#[tauri::command]
pub async fn moderation_log(state: St<'_>, id: String, user_id: Option<String>) -> AppResult<Vec<history::ModerationRow>> {
    Ok(state.history.moderation_log(&id, user_id.as_deref()))
}

// ---------------------------------------------------------------------------------------- player lists

/// Read whitelist/blacklist/admin entries from DedicatedServer.ini.
#[tauri::command]
pub async fn player_lists(state: St<'_>, id: String) -> AppResult<Vec<ListEntry>> {
    let inst = state.instance(&id).await?;
    let path = inst.config_dir().join("DedicatedServer.ini");
    blocking(move || {
        let text = ini::read_file(&path)?;
        if text.is_empty() {
            return Ok(vec![]);
        }
        let doc = ini::IniDoc::parse(&text);
        let section = "/Script/Dominion.DedicatedServerSettings";
        let mut entries = vec![];
        for (key, value) in doc.pairs(section) {
            let base = ini::base_key(&key).to_lowercase();
            if base == "whitelist" || base == "blacklist" || base == "adminlist" {
                let list_name = if base == "whitelist" { "whitelist" } else if base == "blacklist" { "blacklist" } else { "admin" };
                let fields: Vec<(String, String)> = value
                    .trim_start_matches('(').trim_end_matches(')')
                    .split(',')
                    .filter_map(|kv| {
                        let mut parts = kv.splitn(2, '=');
                        let k = parts.next()?.trim().trim_matches('"').to_string();
                        let v = parts.next()?.trim().trim_matches('"').to_string();
                        Some((k, v))
                    })
                    .collect();
                entries.push(ListEntry { list: list_name.to_string(), raw: value.clone(), fields });
            }
        }
        Ok(entries)
    }).await
}

#[tauri::command]
pub async fn remove_list_entry(state: St<'_>, id: String, list: String, raw: String) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    let path = inst.config_dir().join("DedicatedServer.ini");
    let key = match list.as_str() {
        "whitelist" => "Whitelist",
        "blacklist" => "Blacklist",
        _ => "AdminList",
    };
    let raw2 = raw.clone();
    blocking(move || {
        let text = ini::read_file(&path)?;
        let mut doc = ini::IniDoc::parse(&text);
        let section = "/Script/Dominion.DedicatedServerSettings";
        if !doc.remove_value(section, key, &raw2) {
            return Err(AppError::msg("Entry not found in the ini file."));
        }
        ini::write_file(&path, &doc.to_text())?;
        Ok(())
    }).await?;
    state.history.audit(Some(&id), "list-remove", &format!("{list}: {raw}"));
    Ok(())
}

// ---------------------------------------------------------------------------------------- world

/// The live world database. Enhanced uses `game_0.db`; legacy uses `game.db`.
fn live_world(inst: &ServerInstance) -> PathBuf {
    inst.db_path()
}

/// The server's rolling world snapshots (`game_0_backup_N.db` on Enhanced, `game_backup_N.db`
/// on legacy), newest first. The prefix is derived from the live world's file name.
fn world_snapshots(inst: &ServerInstance) -> Vec<PathBuf> {
    let saved = inst.saved_dir();
    // Live world is `game_0.db` (Enhanced) or `game.db` (legacy) → backups are `<base>_backup_N.db`.
    let base = live_world(inst)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = base.strip_suffix(".db").unwrap_or(&base);
    let prefix = format!("{}_backup_", stem);
    let mut out = vec![];
    if let Ok(rd) = std::fs::read_dir(&saved) {
        for e in rd.flatten() {
            let name = e.file_name();
            let ns = name.to_string_lossy();
            if ns.starts_with(&prefix) && ns.ends_with(".db") {
                out.push(e.path());
            }
        }
    }
    out.sort_by(|a, b| {
        let ma = std::fs::metadata(a).and_then(|m| m.modified()).ok();
        let mb = std::fs::metadata(b).and_then(|m| m.modified()).ok();
        mb.cmp(&ma)
    });
    out
}

fn snapshot_of(p: &Path, active: bool) -> Option<WorldSnapshot> {
    let meta = std::fs::metadata(p).ok()?;
    let size = meta.len();
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Some(WorldSnapshot { name, path: p.display().to_string(), size, modified, active })
}

#[tauri::command]
pub async fn list_worlds(state: St<'_>, id: String) -> AppResult<Vec<WorldSnapshot>> {
    let inst = state.instance(&id).await?;
    blocking(move || {
        let mut out: Vec<WorldSnapshot> = vec![];
        let live = live_world(&inst);
        if let Some(s) = snapshot_of(&live, true) {
            out.push(s);
        }
        for p in world_snapshots(&inst) {
            if let Some(s) = snapshot_of(&p, false) {
                out.push(s);
            }
        }
        Ok(out)
    })
    .await
}

#[tauri::command]
pub async fn restore_world(state: St<'_>, id: String, name: String) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    let running = server::running_pids(&inst).1.is_some();
    if running {
        return Err(AppError::msg("Stop the server before restoring a world snapshot."));
    }
    let name2 = name.clone();
    blocking(move || {
        let saved = inst.saved_dir();
        let src = saved.join(&name2);
        if !src.is_file() {
            return Err(AppError::msg("Snapshot not found."));
        }
        let live = live_world(&inst);
        // Quarantine the current live world (and its WAL side files) so it can be recovered.
        let mut to_quarantine = vec![live.clone()];
        for side in ["-wal", "-shm"] {
            let p = PathBuf::from(format!("{}{}", live.display(), side));
            if p.exists() {
                to_quarantine.push(p);
            }
        }
        backup::quarantine_files(&inst, &to_quarantine, "restore-world")?;
        std::fs::copy(&src, &live)?;
        Ok(())
    })
    .await?;
    state.history.audit(Some(&id), "restore-world", &name);
    Ok(())
}

#[tauri::command]
pub async fn export_world(state: St<'_>, id: String, name: String, dest: PathBuf) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    let name2 = name.clone();
    blocking(move || {
        let src = inst.saved_dir().join(&name2);
        if !src.is_file() {
            return Err(AppError::msg("World file not found."));
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&src, &dest)?;
        Ok(())
    })
    .await?;
    state.history.audit(Some(&id), "export-world", &name);
    Ok(())
}

#[tauri::command]
pub async fn import_world(state: St<'_>, id: String, path: PathBuf) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    let running = server::running_pids(&inst).1.is_some();
    if running {
        return Err(AppError::msg("Stop the server before importing a world."));
    }
    let path2 = path.clone();
    blocking(move || {
        if !path2.is_file() {
            return Err(AppError::msg("File not found."));
        }
        let live = live_world(&inst);
        // Quarantine the current live world (and its WAL side files) so it can be recovered.
        let mut to_quarantine = vec![live.clone()];
        for side in ["-wal", "-shm"] {
            let p = PathBuf::from(format!("{}{}", live.display(), side));
            if p.exists() {
                to_quarantine.push(p);
            }
        }
        backup::quarantine_files(&inst, &to_quarantine, "import-world")?;
        std::fs::copy(&path2, &live)?;
        Ok(())
    })
    .await?;
    state.history.audit(Some(&id), "import-world", &path.display().to_string());
    Ok(())
}

#[tauri::command]
pub async fn delete_world(state: St<'_>, id: String, name: String) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    let running = server::running_pids(&inst).1.is_some();
    if running {
        return Err(AppError::msg("Stop the server before deleting a world snapshot."));
    }
    let name2 = name.clone();
    blocking(move || {
        let p = inst.saved_dir().join(&name2);
        if !p.is_file() {
            return Err(AppError::msg("World file not found."));
        }
        // Quarantine instead of hard-delete so it can be recovered.
        backup::quarantine_files(&inst, &[p], "delete-world")?;
        Ok(())
    })
    .await?;
    state.history.audit(Some(&id), "delete-world", &name);
    Ok(())
}

#[tauri::command]
pub async fn reset_world(state: St<'_>, id: String) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    let running = server::running_pids(&inst).1.is_some();
    if running {
        return Err(AppError::msg("Stop the server before resetting the world."));
    }
    blocking(move || {
        let live = live_world(&inst);
        let mut to_quarantine = vec![live.clone()];
        for side in ["-wal", "-shm"] {
            let p = PathBuf::from(format!("{}{}", live.display(), side));
            if p.exists() {
                to_quarantine.push(p);
            }
        }
        to_quarantine.extend(world_snapshots(&inst));
        backup::quarantine_files(&inst, &to_quarantine, "reset-world")?;
        Ok(())
    })
    .await?;
    state.history.audit(Some(&id), "reset-world", "world + snapshots quarantined");
    Ok(())
}

// ---------------------------------------------------------------------------------------- mods

#[tauri::command]
pub async fn list_mods(state: St<'_>, id: String, refresh: bool) -> AppResult<mods::ModsView> {
    let inst = state.instance(&id).await?;
    let ids: Vec<String> = inst.mods.iter().filter(|m| !m.workshop_id.is_empty()).map(|m| m.workshop_id.clone()).collect();
    let info = modsync::workshop_info(&state, &ids, refresh).await;
    let cache = state.workshop_dir().await;
    let (devkit, loaded, overrides, http) = state
        .with_runtime(&id, |r| (r.devkit_revision.unwrap_or(KNOWN_DEVKIT_REVISION), r.mods_loaded.clone(), r.row_overrides.clone(), r.http_requests.clone()))
        .await;
    blocking(move || {
        Ok(mods::build_view(
            &inst,
            &mods::ModContext { cache: &cache, workshop: &info, server_devkit: devkit, loaded: &loaded, row_overrides: &overrides, log_http: &http },
        ))
    })
    .await
}

#[tauri::command]
pub async fn workshop_lookup(state: St<'_>, inputs: Vec<String>) -> AppResult<Vec<workshop::WorkshopItem>> {
    let mut ids = vec![];
    for i in &inputs {
        if let Some(id) = workshop::parse_id(i) {
            ids.push(id);
        }
    }
    if ids.is_empty() {
        return Err(AppError::msg("No Workshop id or link found."));
    }
    Ok(modsync::workshop_info(&state, &ids, true).await.into_values().collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddModsResult {
    added: Vec<String>,
    skipped: Vec<String>,
    /// Workshop items that are required by the added mods but not in the list (from Steam).
    missing_dependencies: Vec<String>,
}

/// Add Workshop mods by id/URL; a collection URL/id adds every item in it. Appended at the end of
/// the load order, enabled. Downloads start in the background.
#[tauri::command]
pub async fn add_mods(app: AppHandle, state: St<'_>, id: String, inputs: Vec<String>, collection: bool) -> AppResult<AddModsResult> {
    let mut inst = state.instance(&id).await?;
    let mut ids: Vec<String> = vec![];
    for i in &inputs {
        let Some(wid) = workshop::parse_id(i) else { continue };
        if collection {
            ids.extend(workshop::collection(&state.http, &wid).await?);
        } else {
            ids.push(wid);
        }
    }
    if ids.is_empty() {
        return Err(AppError::msg("No Workshop id or link found."));
    }
    let info = modsync::workshop_info(&state, &ids, true).await;
    let mut added = vec![];
    let mut skipped = vec![];
    for wid in ids {
        if inst.mods.iter().any(|m| m.workshop_id == wid) {
            skipped.push(format!("{wid} (already in the list)"));
            continue;
        }
        match info.get(&wid) {
            Some(w) if w.found && w.for_conan => {
                inst.mods.push(ModEntry { workshop_id: wid.clone(), enabled: true, name: w.title.clone(), local_path: None });
                added.push(w.title.clone());
            }
            Some(w) if w.found => skipped.push(format!("{} (not a Conan Exiles mod)", w.title)),
            _ => skipped.push(format!("{wid} (not found on the Workshop)")),
        }
    }
    state.put_instance(&inst).await?;
    state.history.audit(Some(&id), "mods-add", &added.join(", "));
    // Dependencies (best effort).
    let api_key = state.settings.read().await.steam_api_key.clone();
    let mut missing_dependencies = vec![];
    for m in inst.mods.iter().rev().take(added.len()) {
        if let Ok(deps) = workshop::dependencies(&state.http, &api_key, &m.workshop_id).await {
            for d in deps {
                if !inst.mods.iter().any(|x| x.workshop_id == d) && !missing_dependencies.contains(&d) {
                    missing_dependencies.push(d);
                }
            }
        }
    }
    if !added.is_empty() {
        let (st, app2, i2) = (state.inner().clone(), app.clone(), inst.clone());
        background(&app, &id, "Mod download", async move { modsync::sync(&st, &app2, &i2, false, true).await.map(|_| ()) });
    }
    let _ = app.emit("instances-changed", ());
    Ok(AddModsResult { added, skipped, missing_dependencies })
}

#[tauri::command]
pub async fn add_local_mod(app: AppHandle, state: St<'_>, id: String, path: PathBuf) -> AppResult<ModEntry> {
    let mut inst = state.instance(&id).await?;
    if !path.is_file() || path.extension().map(|x| !x.eq_ignore_ascii_case("pak")).unwrap_or(true) {
        return Err(AppError::msg("Pick a .pak file."));
    }
    if inst.mods.iter().any(|m| m.local_path.as_ref().map(|p| proc::norm(p) == proc::norm(&path)).unwrap_or(false)) {
        return Err(AppError::msg("That pak is already in the list."));
    }
    let p2 = path.clone();
    let info = blocking(move || Ok(mods::read_modinfo(&p2))).await?;
    let name = info.map(|i| i.name).filter(|n| !n.is_empty()).unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().into_owned());
    let entry = ModEntry { workshop_id: String::new(), local_path: Some(path), enabled: true, name };
    inst.mods.push(entry.clone());
    state.put_instance(&inst).await?;
    state.history.audit(Some(&id), "mods-add-local", &entry.name);
    let _ = app.emit("instances-changed", ());
    Ok(entry)
}

/// Replace the mod list (reorder, enable/disable, remove) in one go.
#[tauri::command]
pub async fn set_mods(app: AppHandle, state: St<'_>, id: String, mods_list: Vec<ModEntry>) -> AppResult<()> {
    let mut inst = state.instance(&id).await?;
    let changed = inst.mods != mods_list;
    inst.mods = mods_list;
    state.put_instance(&inst).await?;
    if changed {
        state.history.audit(Some(&id), "mods-set", &inst.mods.iter().map(|m| format!("{}{}", if m.enabled { "" } else { "-" }, m.key())).collect::<Vec<_>>().join(","));
        let running = server::running_pids(&inst).1.is_some();
        if running {
            state.with_runtime(&id, |r| r.pending_restart = true).await;
        } else {
            let cache = state.workshop_dir().await;
            let i2 = inst.clone();
            blocking(move || {
                mods::write_modlist(&cache, &i2)?;
                server::sync_ini(&i2).map(|_| ())
            })
            .await?;
        }
    }
    state.emit_status(&app, &id).await;
    let _ = app.emit("instances-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn update_mods(app: AppHandle, state: St<'_>, id: String, restart: bool) -> AppResult<()> {
    let st = state.inner().clone();
    let (app2, id2) = (app.clone(), id.clone());
    state.history.audit(Some(&id), "mods-update", "");
    background(&app, &id, "Mod update", async move {
        if restart {
            supervisor::update_mods_job(&st, &app2, &id2, "mod update").await
        } else {
            let inst = st.instance(&id2).await?;
            modsync::sync(&st, &app2, &inst, true, false).await?;
            let _ = app2.emit("mods-changed", &id2);
            Ok(())
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn check_mod_updates(app: AppHandle, state: St<'_>, id: String) -> AppResult<Vec<String>> {
    let st = state.inner().clone();
    supervisor::check_mod_updates(&st, &app, &id, false).await
}

#[tauri::command]
pub async fn mod_versions(state: St<'_>, workshop_id: String) -> AppResult<Vec<mods::ArchivedVersion>> {
    let cache = state.workshop_dir().await;
    blocking(move || Ok(mods::archived_versions(&cache, &workshop_id))).await
}

#[tauri::command]
pub async fn rollback_mod(state: St<'_>, id: String, workshop_id: String, stamp: i64) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    require_stopped(&inst)?;
    let cache = state.workshop_dir().await;
    state.history.audit(Some(&id), "mod-rollback", &format!("{workshop_id}@{stamp}"));
    blocking(move || mods::rollback(&cache, &workshop_id, stamp)).await
}

#[tauri::command]
pub async fn set_http_origin(state: St<'_>, id: String, mod_folder: String, origin: String, allow: bool) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    state.history.audit(Some(&id), if allow { "http-allow" } else { "http-deny" }, &format!("{mod_folder} {origin}"));
    let running = server::running_pids(&inst).1.is_some();
    blocking(move || mods::set_http_origin(&inst, &mod_folder, &origin, allow)).await?;
    if running {
        state.with_runtime(&id, |r| r.pending_restart = true).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn clean_stray_mods(state: St<'_>, id: String) -> AppResult<Vec<String>> {
    let inst = state.instance(&id).await?;
    let mods_path = inst.project_dir().join("Content").join("Paks").join("~mods");
    if !mods_path.is_dir() {
        return Ok(vec![]);
    }
    let running = server::running_pids(&inst).1.is_some();
    if running {
        return Err(AppError::msg("Stop the server before removing the ~mods folder."));
    }
    let removed = blocking(move || {
        let mut names = vec![];
        if let Ok(rd) = std::fs::read_dir(&mods_path) {
            for e in rd.flatten() {
                names.push(e.file_name().to_string_lossy().into_owned());
            }
        }
        std::fs::remove_dir_all(&mods_path)?;
        // Also clean up the parent Paks dir if it's now empty (it was created for ~mods).
        let paks = mods_path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        if paks.is_dir() && paks.read_dir().map(|d| d.count() == 0).unwrap_or(false) {
            let _ = std::fs::remove_dir(paks);
        }
        Ok::<_, AppError>(names)
    }).await?;
    state.history.audit(Some(&id), "clean-mods", &format!("{} items removed", removed.len()));
    Ok(removed)
}

#[tauri::command]
pub async fn workshop_search(state: St<'_>, text: String, page: u32, enhanced_only: bool) -> AppResult<Vec<workshop::WorkshopItem>> {
    let key = state.settings.read().await.steam_api_key.clone();
    workshop::search(&state.http, &key, &text, page, enhanced_only).await
}

#[tauri::command]
pub async fn save_mod_profile(app: AppHandle, state: St<'_>, id: String, name: String) -> AppResult<ModProfile> {
    let mut inst = state.instance(&id).await?;
    let p = ModProfile { id: uuid::Uuid::new_v4().to_string(), name: name.trim().to_string(), mods: inst.mods.clone() };
    inst.mod_profiles.retain(|x| !x.name.eq_ignore_ascii_case(&p.name));
    inst.mod_profiles.push(p.clone());
    state.put_instance(&inst).await?;
    let _ = app.emit("instances-changed", ());
    Ok(p)
}

#[tauri::command]
pub async fn apply_mod_profile(app: AppHandle, state: St<'_>, id: String, profile_id: String) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    let p = inst.mod_profiles.iter().find(|p| p.id == profile_id).cloned().ok_or_else(|| AppError::msg("Mod profile not found"))?;
    state.history.audit(Some(&id), "mod-profile", &p.name);
    set_mods(app, state, id, p.mods).await
}

#[tauri::command]
pub async fn delete_mod_profile(app: AppHandle, state: St<'_>, id: String, profile_id: String) -> AppResult<()> {
    let mut inst = state.instance(&id).await?;
    inst.mod_profiles.retain(|p| p.id != profile_id);
    state.put_instance(&inst).await?;
    let _ = app.emit("instances-changed", ());
    Ok(())
}

// ------------------------------------------------------------------------------------- backups

#[tauri::command]
pub async fn create_backup(app: AppHandle, state: St<'_>, id: String, label: String, full: bool) -> AppResult<BackupInfo> {
    let inst = state.instance(&id).await?;
    state.set_busy(&app, &id, Some("Backing up")).await;
    let res = backup::create(&state, &inst, if label.trim().is_empty() { "Manual backup" } else { label.trim() }, if full { "full" } else { "manual" }).await;
    state.set_busy(&app, &id, None).await;
    let b = res?;
    state.with_runtime(&id, |r| r.last_backup = b.created_at).await;
    state.history.audit(Some(&id), "backup", &b.name);
    state.history.event(Some(&id), "success", "backup", &format!("Backup {} created", b.name));
    let _ = app.emit("backup-created", &b);
    Ok(b)
}

#[tauri::command]
pub async fn list_backups(state: St<'_>, id: String) -> AppResult<Vec<BackupInfo>> {
    let inst = state.instance(&id).await?;
    let dir = state.backup_dir(&inst).await;
    blocking(move || Ok(backup::list(&dir))).await
}

#[tauri::command]
pub async fn get_backup_dir(state: St<'_>, id: String) -> AppResult<PathBuf> {
    let inst = state.instance(&id).await?;
    Ok(state.backup_dir(&inst).await)
}

#[tauri::command]
pub async fn verify_backup(state: St<'_>, id: String, path: PathBuf) -> AppResult<backup::VerifyReport> {
    let inst = state.instance(&id).await?;
    let dir = state.backup_dir(&inst).await;
    if !proc::is_within(&path, &dir) {
        return Err(AppError::msg("That backup isn't in this server's backup folder."));
    }
    let tmp = state.data_dir.join("tmp");
    blocking(move || backup::verify(&path, &tmp)).await
}

#[tauri::command]
pub async fn restore_backup(app: AppHandle, state: St<'_>, id: String, path: PathBuf, include_config: bool, restore_mods: bool) -> AppResult<u32> {
    let inst = state.instance(&id).await?;
    require_stopped(&inst)?;
    let dir = state.backup_dir(&inst).await;
    if !proc::is_within(&path, &dir) {
        return Err(AppError::msg("That backup isn't in this server's backup folder."));
    }
    // Safety net: snapshot what's there now.
    if inst.db_path().exists() {
        let _ = backup::create(&state, &inst, "Before restore", "auto").await;
    }
    state.history.audit(Some(&id), "restore", &path.display().to_string());
    let (i2, p2) = (inst.clone(), path.clone());
    let n = blocking(move || backup::restore(&i2, &p2, include_config)).await?;
    let mut updated = inst.clone();
    if include_config {
        server::read_ini_into(&mut updated);
        // RCON credentials stay the manager's own.
        updated.rcon = inst.rcon.clone();
    }
    if restore_mods {
        if let Some(list) = backup::backup_mod_list(&path) {
            updated.mods = list;
        }
    }
    state.put_instance(&updated).await?;
    let _ = app.emit("instances-changed", ());
    manager_log(&app, &id, format!("Restored {n} files from backup."));
    state.history.event(Some(&id), "success", "restore", &format!("Restored {}", path.display()));
    Ok(n)
}

#[tauri::command]
pub async fn delete_backup(state: St<'_>, id: String, path: PathBuf) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    let dir = state.backup_dir(&inst).await;
    if !proc::is_within(&path, &dir) {
        return Err(AppError::msg("That backup isn't in this server's backup folder."));
    }
    state.history.audit(Some(&id), "delete-backup", &path.display().to_string());
    std::fs::remove_file(path)?;
    Ok(())
}

// --------------------------------------------------------------------------------------- packs

#[tauri::command]
pub async fn export_pack(state: St<'_>, id: String, dest: PathBuf, options: backup::PackOptions) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    let tmp = state.data_dir.join("tmp");
    state.history.audit(Some(&id), "export-pack", &dest.display().to_string());
    blocking(move || backup::export_pack(&inst, &dest, &options, &tmp)).await
}

#[tauri::command]
pub async fn read_pack(path: PathBuf) -> AppResult<PackManifest> {
    blocking(move || backup::read_pack(&path)).await
}

#[tauri::command]
pub async fn import_pack(app: AppHandle, state: St<'_>, path: PathBuf, name: String, install_dir: Option<PathBuf>, install_now: bool) -> AppResult<ServerInstance> {
    let p2 = path.clone();
    let m = blocking(move || backup::read_pack(&p2)).await?;
    let mut inst = m.instance;
    inst.id = uuid::Uuid::new_v4().to_string();
    inst.created_at = now_ms();
    inst.name = if name.trim().is_empty() { inst.name } else { name.trim().to_string() };
    inst.install_dir = match install_dir {
        Some(d) if !d.as_os_str().is_empty() => abs_dir(d)?,
        _ => unique_dir(&state, &inst.name).await,
    };
    let list = state.instances.read().await.clone();
    let free = network::suggest_ports(&list, None).await;
    inst.launch.port = free.game;
    inst.launch.query_port = free.query;
    inst.rcon.port = free.rcon;
    if inst.rcon.password.is_empty() {
        inst.rcon.password = secrets::random_password(24);
    }
    let i2 = inst.clone();
    blocking(move || {
        std::fs::create_dir_all(&i2.install_dir)?;
        backup::unpack_into(&path, &i2)?;
        server::sync_ini(&i2).map(|_| ())
    })
    .await?;
    let inst = add_instance(&state, inst).await?;
    if install_now && !inst.is_installed() {
        let (st, app2, id) = (state.inner().clone(), app.clone(), inst.id.clone());
        background(&app, &inst.id, "Install", async move { server::update_job(&st, &app2, &id, false).await });
    }
    state.emit_status(&app, &inst.id).await;
    Ok(inst)
}

// ------------------------------------------------------------------------------------- network

#[tauri::command]
pub async fn firewall_state(state: St<'_>, id: String) -> AppResult<network::FirewallState> {
    let inst = state.instance(&id).await?;
    Ok(network::firewall_state(&inst).await)
}

#[tauri::command]
pub async fn add_firewall_rules(state: St<'_>, id: String) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    state.history.audit(Some(&id), "firewall-add", "");
    let out = network::add_firewall_rules(&inst, &state.data_dir.join("tmp")).await?;
    if !out.contains("RULES_OK") {
        return Err(AppError::msg(format!("Windows didn't confirm the rules: {}", out.lines().filter(|l| l.contains("ERROR") || l.contains("Error")).collect::<Vec<_>>().join(" "))));
    }
    Ok(())
}

#[tauri::command]
pub async fn remove_firewall_rules(state: St<'_>, id: String) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    state.history.audit(Some(&id), "firewall-remove", "");
    network::remove_firewall_rules(&inst, &state.data_dir.join("tmp")).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetInfo {
    public_ip: Option<String>,
    local: Vec<network::LocalIp>,
    gateway: Option<String>,
}

#[tauri::command]
pub async fn net_info(state: St<'_>) -> AppResult<NetInfo> {
    let (public_ip, gateway) = tokio::join!(network::public_ip(&state.http), network::default_gateway());
    let mut local = blocking(|| Ok(network::local_ips())).await?;
    if let Some(gw) = &gateway {
        let prefix: String = gw.rsplitn(2, '.').nth(1).unwrap_or("").to_string() + ".";
        local.sort_by_key(|l| !l.ip.starts_with(&prefix));
    }
    Ok(NetInfo { public_ip, local, gateway })
}

#[tauri::command]
pub async fn upnp_probe(state: St<'_>) -> AppResult<crate::core::upnp::UpnpInfo> {
    Ok(crate::core::upnp::probe(&state.http).await)
}

#[tauri::command]
pub async fn upnp_map(app: AppHandle, state: St<'_>, id: String) -> AppResult<Vec<String>> {
    let inst = state.instance(&id).await?;
    let lines = crate::core::upnp::map_server(&inst).await?;
    manager_log(&app, &id, format!("UPnP: {}", lines.join(", ")));
    Ok(lines)
}

#[tauri::command]
pub async fn upnp_unmap(state: St<'_>, id: String) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    crate::core::upnp::unmap_server(&inst).await
}

// --------------------------------------------------------------------------------- diagnostics

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagReport {
    checks: Vec<DiagCheck>,
    score: u32,
}

#[tauri::command]
pub async fn run_diagnostics(state: St<'_>, id: String) -> AppResult<DiagReport> {
    let inst = state.instance(&id).await?;
    let checks = diagnostics::run(&state, &inst).await;
    Ok(DiagReport { score: diagnostics::score(&checks), checks })
}

#[tauri::command]
pub async fn fix_build_override(state: St<'_>, id: String) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    require_stopped(&inst)?;
    blocking(move || server::sync_ini(&inst).map(|_| ())).await
}

#[tauri::command]
pub async fn install_prereqs(state: St<'_>, id: String) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    let exe = match diagnostics::prereq_installer(&inst) {
        Some(p) => p,
        None => {
            let tmp = state.data_dir.join("tmp");
            std::fs::create_dir_all(&tmp)?;
            let exe = tmp.join("vc_redist.x64.exe");
            crate::core::download::download(&state.http, "https://aka.ms/vs/17/release/vc_redist.x64.exe", &exe, None, "Visual C++ runtime").await?;
            exe
        }
    };
    let script = format!("Start-Process -FilePath '{}' -ArgumentList '/install','/quiet','/norestart' -Wait", exe.display());
    proc::powershell_elevated(&script, &state.data_dir.join("tmp")).await?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskUsage {
    install: u64,
    world: u64,
    backups: u64,
    logs: u64,
    mods: u64,
}

#[tauri::command]
pub async fn disk_usage(state: St<'_>, id: String) -> AppResult<DiskUsage> {
    let inst = state.instance(&id).await?;
    let bk = state.backup_dir(&inst).await;
    let cache = state.workshop_dir().await;
    blocking(move || {
        let backups = backup::folder_size(&bk);
        let install = backup::folder_size(&inst.install_dir);
        let inside = proc::is_within(&bk, &inst.install_dir);
        let db = inst.db_path();
        let world = [db.clone(), PathBuf::from(format!("{}-wal", db.display()))].iter().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum();
        Ok(DiskUsage {
            install: if inside { install.saturating_sub(backups) } else { install },
            world,
            backups,
            logs: backup::folder_size(&inst.logs_dir()),
            mods: mods::total_size(&cache, &inst),
        })
    })
    .await
}
