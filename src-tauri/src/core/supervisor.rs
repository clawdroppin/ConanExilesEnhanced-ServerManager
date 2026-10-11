//! The single background loop: process tracking (incl. servers started outside the manager),
//! state transitions, crash & hang detection with auto-restart, telemetry + 1-minute metric
//! rollups, log tail (ready/version/devkit/mods/conflicts/HTTP requests), RCON player polling
//! (join/leave + sessions), schedules, the rules engine, interval backups and update checks.

use super::{backup, modsync, server, steamcmd};
use crate::core::history::MetricRow;
use crate::models::*;
use crate::state::{manager_log, notify, now_ms, AppState, TaskReporter};
use chrono::{Datelike, Local, NaiveTime, TimeZone, Timelike};
use regex::Regex;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use sysinfo::{Networks, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use tauri::{AppHandle, Emitter, Manager};

// Every pattern below was captured from a real Enhanced server log (see tests).
static READY_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(LogServerStats: Startup report|Started SourceServerQueries on port)").unwrap());
static BUILD_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"LogInit: Build: (\S+)").unwrap());
static PROJECT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"LogNetVersion: Set ProjectVersion to ([0-9]+(?:\.[0-9]+)*)").unwrap());
static DEVKIT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"SetCompatibleDevkitVersions: .*?\[(\d+)").unwrap());
static MOD_LOADED_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"LogModManager: Mod '([^']+)' contributes").unwrap());
static CONTROLLER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"AddActiveModControllerClass: /Game/(Mods|DLC)/([^/]+)/").unwrap());
static MERGE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"MergeDataTables - Found existing table entry for \S*?\.(\w+) with RowName: ([^,]+), overwriting").unwrap());
static HTTP_SECTION_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"LogModHttp: Warning: \[([^\]]+)\] ; mod "(.*)""#).unwrap());
static HTTP_ORIGIN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"LogModHttp: Warning: AllowedOrigin="([^"]+)"(?: ; reason: "(.*)")?"#).unwrap());
static FATAL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(Fatal error|appError called|Assertion failed:|Unhandled Exception|LogWindows: Error: === Critical error)").unwrap());

const CRASH_WINDOW_MS: i64 = 15 * 60 * 1000;
const MAX_LINES_PER_TICK: usize = 400;

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state: Arc<AppState> = app.state::<Arc<AppState>>().inner().clone();
        let mut sup = Supervisor::new();
        sup.boot(&state, &app).await;
        loop {
            let interval = state.settings.read().await.telemetry_interval_ms.clamp(500, 5000);
            let t0 = Instant::now();
            sup.tick(&state, &app).await;
            let spent = t0.elapsed();
            tokio::time::sleep(Duration::from_millis(interval).saturating_sub(spent)).await;
        }
    });
}

#[derive(Default)]
struct Rollup {
    minute: i64,
    n: u32,
    cpu: f64,
    mem: u64,
    read: u64,
    write: u64,
    players: u32,
}

/// Something that happened this tick, fed to the rules engine.
#[derive(Debug, Clone, PartialEq)]
enum Fact {
    Crash,
    Started,
    Stopped,
    Join,
    Leave,
    UpdateAvailable,
    ModUpdateAvailable,
    Log(String),
}

struct Supervisor {
    sys: System,
    nets: Networks,
    last_tick: Instant,
    last_status: HashMap<String, String>,
    over_mem: HashMap<String, u32>,
    port_owners: HashMap<u16, u32>,
    last_port_scan: i64,
    hide_until: HashMap<String, i64>,
    since_scan: u32,
    tracked: Vec<sysinfo::Pid>,
    rollups: HashMap<String, Rollup>,
    last_sample: HashMap<String, ProcSample>,
    regex_cache: HashMap<String, Option<Regex>>,
    last_prune: i64,
    polling: HashMap<String, bool>,
}

impl Supervisor {
    fn new() -> Self {
        Self {
            sys: System::new(),
            nets: Networks::new_with_refreshed_list(),
            last_tick: Instant::now(),
            last_status: HashMap::new(),
            over_mem: HashMap::new(),
            port_owners: HashMap::new(),
            last_port_scan: 0,
            hide_until: HashMap::new(),
            since_scan: u32::MAX,
            tracked: vec![],
            rollups: HashMap::new(),
            last_sample: HashMap::new(),
            regex_cache: HashMap::new(),
            last_prune: 0,
            polling: HashMap::new(),
        }
    }

    async fn boot(&mut self, state: &Arc<AppState>, app: &AppHandle) {
        let instances = state.instances.read().await.clone();
        let check_updates = state.settings.read().await.check_updates_on_launch;
        for inst in &instances {
            let build = steamcmd::local_build(&inst.install_dir);
            let last_bk = backup::list(&state.backup_dir(inst).await).first().map(|b| b.created_at).unwrap_or(0);
            // Sessions left open by a previous run can't be trusted.
            state.history.close_all_sessions(&inst.id);
            state
                .with_runtime(&inst.id, |r| {
                    r.installed_build = build;
                    r.last_backup = last_bk;
                    // 0 = check soon after launch; otherwise wait a full interval.
                    r.last_update_check = if check_updates { 0 } else { now_ms() };
                    r.last_mod_check = if check_updates { now_ms() - 50 * 60_000 } else { now_ms() };
                })
                .await;
        }
        let lost = state.lost_secrets.lock().await.clone();
        if !lost.is_empty() {
            notify(app, "", "warn", format!("Some saved secrets couldn't be decrypted on this PC/account and must be re-entered: {}", lost.join(", ")));
        }
        self.sys.refresh_cpu_usage();
        self.refresh_procs();
        tokio::time::sleep(Duration::from_millis(300)).await;
        for inst in instances.into_iter().filter(|i| i.automation.auto_start && i.is_installed()) {
            let (_, main) = server::running_pids(&inst);
            if main.is_none() {
                let (st, app2) = (state.clone(), app.clone());
                tauri::async_runtime::spawn(async move {
                    manager_log(&app2, &inst.id, "Auto-starting with the manager…");
                    if let Err(e) = server::start(&st, &app2, &inst.id).await {
                        notify(&app2, &inst.id, "error", format!("Auto-start of {} failed: {e}", inst.name));
                    }
                });
            }
        }
    }

    /// Full process discovery (names + exe paths) every 3 ticks; CPU/memory/disk only for the
    /// Conan server processes.
    fn refresh_procs(&mut self) {
        if self.since_scan >= 3 {
            self.sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet));
            self.tracked = self
                .sys
                .processes()
                .iter()
                .filter(|(_, p)| p.name().to_string_lossy().to_lowercase().starts_with("conansandboxserver"))
                .map(|(pid, _)| *pid)
                .collect();
            self.since_scan = 0;
        }
        self.since_scan += 1;
        if !self.tracked.is_empty() {
            let pids = self.tracked.clone();
            self.sys.refresh_processes_specifics(
                ProcessesToUpdate::Some(&pids),
                true,
                ProcessRefreshKind::nothing().with_cpu().with_memory().with_disk_usage().with_exe(UpdateKind::OnlyIfNotSet),
            );
            self.tracked.retain(|p| self.sys.process(*p).is_some());
        }
    }

    fn rescan_soon(&mut self) {
        self.since_scan = u32::MAX;
    }

    async fn tick(&mut self, state: &Arc<AppState>, app: &AppHandle) {
        let elapsed = self.last_tick.elapsed().as_secs_f64().max(0.2);
        self.last_tick = Instant::now();
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.refresh_procs();
        self.nets.refresh(true);
        let cores = self.sys.cpus().len().max(1);
        let (rx, tx) = self.nets.iter().fold((0u64, 0u64), |(r, t), (_, d)| (r + d.received(), t + d.transmitted()));
        let system = SystemSample {
            cpu: self.sys.global_cpu_usage(),
            mem_used: self.sys.used_memory(),
            mem_total: self.sys.total_memory(),
            rx_bps: (rx as f64 / elapsed) as u64,
            tx_bps: (tx as f64 / elapsed) as u64,
            cores,
            per_core: self.sys.cpus().iter().map(|c| c.cpu_usage()).collect(),
        };

        let procs: Vec<(u32, String)> = self
            .tracked
            .iter()
            .filter_map(|pid| self.sys.process(*pid).and_then(|p| p.exe()).map(|e| (pid.as_u32(), super::proc::norm(e))))
            .collect();

        let now = now_ms();
        let any_starting = state.runtime.lock().await.values().any(|r| r.state == Some(ServerState::Starting));
        if any_starting {
            self.rescan_soon();
        }
        if any_starting && now - self.last_port_scan > 2000 {
            self.port_owners = super::proc::udp_port_owners().await;
            self.last_port_scan = now;
        }

        let instances = state.instances.read().await.clone();
        let mut samples = vec![];
        for inst in &instances {
            let base = format!("{}\\", super::proc::norm(&inst.install_dir));
            let mine: Vec<&(u32, String)> = procs.iter().filter(|(_, e)| e.starts_with(&base)).collect();
            let pids: Vec<u32> = mine.iter().map(|(p, _)| *p).collect();
            let main = mine.iter().find(|(_, e)| e.contains("shipping")).or_else(|| mine.first()).map(|(p, _)| *p);
            let mut facts: Vec<Fact> = vec![];

            if !pids.is_empty() {
                let mut s = ProcSample { id: inst.id.clone(), ..Default::default() };
                for pid in &pids {
                    if let Some(p) = self.sys.process(sysinfo::Pid::from_u32(*pid)) {
                        s.cpu += p.cpu_usage() / cores as f32;
                        s.mem += p.memory();
                        s.virt += p.virtual_memory();
                        let du = p.disk_usage();
                        s.read_bps += (du.read_bytes as f64 / elapsed) as u64;
                        s.write_bps += (du.written_bytes as f64 / elapsed) as u64;
                        s.uptime = s.uptime.max(p.run_time());
                    }
                }
                self.resource_limits(state, app, inst, &s).await;
                self.rollup(state, inst, &s, now).await;
                self.last_sample.insert(inst.id.clone(), s.clone());
                samples.push(s);
            } else {
                self.over_mem.remove(&inst.id);
                self.last_sample.remove(&inst.id);
            }

            self.lifecycle(state, app, inst, &pids, main, now, &mut facts).await;
            self.tail_log(state, app, inst, &mut facts).await;
            self.hide_console(state, inst, &pids, now).await;
            self.poll_players(state, app, inst, now, &mut facts).await;
            self.hang_check(state, app, inst, now).await;
            self.automation(state, app, inst, main.is_some(), now, &mut facts).await;
            self.rules(state, app, inst, now, &facts).await;

            let st = state.status(inst).await;
            let key = serde_json::to_string(&st).unwrap_or_default();
            if self.last_status.get(&inst.id) != Some(&key) {
                self.last_status.insert(inst.id.clone(), key);
                let _ = app.emit("server-status", st);
            }
        }
        let _ = app.emit("telemetry", TelemetryEvent { ts: now, system, servers: samples });
        if now - self.last_prune > 6 * 3_600_000 {
            self.last_prune = now;
            let days = state.settings.read().await.metrics_retention_days;
            state.history.prune(days);
        }
    }

    /// Average the per-second samples into one row per minute in manager.db.
    async fn rollup(&mut self, state: &Arc<AppState>, inst: &ServerInstance, s: &ProcSample, now: i64) {
        let minute = now - now % 60_000;
        let players = state.with_runtime(&inst.id, |r| r.players.len() as u32).await;
        let r = self.rollups.entry(inst.id.clone()).or_default();
        if r.minute != 0 && r.minute != minute && r.n > 0 {
            let db_size = std::fs::metadata(inst.db_path()).map(|m| m.len()).unwrap_or(0) as i64;
            state.history.metric(
                &inst.id,
                &MetricRow {
                    ts: r.minute,
                    cpu: r.cpu / r.n as f64,
                    mem: r.mem as i64,
                    read_bps: (r.read / r.n as u64) as i64,
                    write_bps: (r.write / r.n as u64) as i64,
                    players: r.players as i64,
                    db_size,
                },
            );
            *r = Rollup::default();
        }
        r.minute = minute;
        r.n += 1;
        r.cpu += s.cpu as f64;
        r.mem = r.mem.max(s.mem);
        r.read += s.read_bps;
        r.write += s.write_bps;
        r.players = r.players.max(players);
    }

    #[allow(clippy::too_many_arguments)]
    async fn lifecycle(&mut self, state: &Arc<AppState>, app: &AppHandle, inst: &ServerInstance, pids: &[u32], main: Option<u32>, now: i64, facts: &mut Vec<Fact>) {
        let lock = state.op_lock(&inst.id).await;
        let Ok(_guard) = lock.try_lock() else { return };
        let alive = main.is_some();
        let port_bound = pids.iter().any(|p| self.port_owners.get(&inst.launch.query_port) == Some(p));
        let start_time = main.and_then(|p| self.sys.process(sysinfo::Pid::from_u32(p))).map(|p| p.start_time() as i64 * 1000);

        enum Ev {
            None,
            Adopted,
            Ready,
            Crashed { restart_in: Option<u32>, looped: bool },
            Stopped,
        }
        let ev = state
            .with_runtime(&inst.id, |r| {
                let cur = r.state.unwrap_or(ServerState::Stopped);
                match (alive, cur) {
                    (true, ServerState::Starting) => {
                        r.pid = main;
                        // The query port only opens once the map is loaded (verified) – that, or
                        // the "Startup report" log line, means players can join.
                        let slow = r.started_at.map(|t| now - t > 600_000).unwrap_or(false);
                        if r.log_ready || port_bound || slow {
                            r.state = Some(ServerState::Running);
                            r.ready_at = Some(now);
                            return Ev::Ready;
                        }
                        Ev::None
                    }
                    (true, ServerState::Running) => {
                        r.pid = main;
                        Ev::None
                    }
                    (true, ServerState::Stopping | ServerState::Restarting | ServerState::Updating | ServerState::Installing) => Ev::None,
                    (true, _) => {
                        r.pid = main;
                        r.state = Some(ServerState::Running);
                        r.started_at = start_time.or(Some(now));
                        r.ready_at = Some(now);
                        r.intentional_stop = false;
                        r.restart_at = None;
                        r.last_log_at = now;
                        r.log_offset = std::fs::metadata(inst.log_path()).map(|m| m.len()).unwrap_or(0);
                        Ev::Adopted
                    }
                    (false, ServerState::Starting) if r.started_at.map(|t| now - t < 10_000).unwrap_or(false) => Ev::None,
                    (false, ServerState::Starting | ServerState::Running | ServerState::Stopping) => {
                        r.pid = None;
                        r.started_at = None;
                        r.ready_at = None;
                        r.players.clear();
                        r.rcon_ok = false;
                        if r.intentional_stop || cur == ServerState::Stopping {
                            r.state = Some(ServerState::Stopped);
                            return Ev::Stopped;
                        }
                        r.state = Some(ServerState::Crashed);
                        r.crash_times.retain(|t| now - t < CRASH_WINDOW_MS);
                        r.crash_times.push(now);
                        let looped = inst.automation.crash_limit > 0 && r.crash_times.len() as u32 > inst.automation.crash_limit;
                        r.crash_loop = looped;
                        let restart_in = (inst.automation.auto_restart && !looped).then_some(inst.automation.restart_delay_secs);
                        if let Some(d) = restart_in {
                            r.restart_at = Some(now + d as i64 * 1000);
                        }
                        Ev::Crashed { restart_in, looped }
                    }
                    _ => Ev::None,
                }
            })
            .await;
        match ev {
            Ev::Adopted => {
                manager_log(app, &inst.id, format!("Found the server already running (PID {}) – now monitoring it.", main.unwrap_or(0)));
                let meta = scan_log_meta(&inst.log_path());
                state
                    .with_runtime(&inst.id, |r| {
                        r.engine_build = meta.build;
                        r.game_version = meta.project;
                        r.devkit_revision = meta.devkit;
                        r.mods_loaded = meta.mods;
                    })
                    .await;
                facts.push(Fact::Started);
            }
            Ev::Ready => {
                manager_log(app, &inst.id, format!("Server is online – game UDP {}, query UDP {}.", inst.launch.port, inst.launch.query_port));
                notify(app, &inst.id, "success", format!("{} is online", inst.name));
                facts.push(Fact::Started);
            }
            Ev::Crashed { restart_in, looped } => {
                let msg = match (restart_in, looped) {
                    (_, true) => format!("{} keeps crashing – auto-restart paused. Check the console and Health.", inst.name),
                    (Some(d), _) => format!("{} stopped unexpectedly – restarting in {d}s", inst.name),
                    (None, _) => format!("{} stopped unexpectedly", inst.name),
                };
                manager_log(app, &inst.id, &msg);
                notify(app, &inst.id, "error", msg);
                state.rcon.disconnect(&inst.id).await;
                state.history.close_all_sessions(&inst.id);
                facts.push(Fact::Crash);
            }
            Ev::Stopped => {
                state.rcon.disconnect(&inst.id).await;
                state.history.close_all_sessions(&inst.id);
                facts.push(Fact::Stopped);
            }
            Ev::None => {}
        }
        if matches!(ev, Ev::Crashed { .. } | Ev::Stopped) && server::apply_pending(state, inst).await.unwrap_or(false) {
            manager_log(app, &inst.id, "Applied config changes saved while the server was running.");
        }

        let due = state
            .with_runtime(&inst.id, |r| match r.restart_at {
                Some(t) if t <= now && !alive => {
                    r.restart_at = None;
                    true
                }
                _ => false,
            })
            .await;
        if due {
            let (st, app2, id) = (state.clone(), app.clone(), inst.id.clone());
            tauri::async_runtime::spawn(async move {
                manager_log(&app2, &id, "Auto-restarting after crash…");
                if let Err(e) = server::start(&st, &app2, &id).await {
                    notify(&app2, &id, "error", format!("Auto-restart failed: {e}"));
                }
            });
        }
    }

    async fn hide_console(&mut self, state: &Arc<AppState>, inst: &ServerInstance, pids: &[u32], now: i64) {
        if inst.launch.show_console || pids.is_empty() {
            self.hide_until.remove(&inst.id);
            return;
        }
        let started = state.with_runtime(&inst.id, |r| r.started_at.unwrap_or(0)).await;
        let until = *self.hide_until.entry(inst.id.clone()).or_insert(started.max(now - 1) + 60_000);
        if now < until {
            let hidden_now = state.with_runtime(&inst.id, |r| r.console_hidden).await;
            if hidden_now {
                super::proc::set_console_visible(pids, false);
            }
        }
    }

    async fn resource_limits(&mut self, state: &Arc<AppState>, app: &AppHandle, inst: &ServerInstance, s: &ProcSample) {
        let a = &inst.automation;
        let running = state.with_runtime(&inst.id, |r| r.state == Some(ServerState::Running) && r.restart_pending_at.is_none()).await;
        if !running {
            return;
        }
        let mut reason = None;
        if a.memory_limit_mb > 0 && s.mem > a.memory_limit_mb as u64 * 1_048_576 {
            let n = self.over_mem.entry(inst.id.clone()).or_insert(0);
            *n += 1;
            if *n >= 30 {
                reason = Some(format!("memory above {} MB", a.memory_limit_mb));
            }
        } else {
            self.over_mem.remove(&inst.id);
        }
        if a.max_uptime_hours > 0 && s.uptime >= a.max_uptime_hours as u64 * 3600 {
            reason = Some(format!("uptime reached {} h", a.max_uptime_hours));
        }
        if let Some(why) = reason {
            self.over_mem.remove(&inst.id);
            let (st, app2, id, name, warn) = (state.clone(), app.clone(), inst.id.clone(), inst.name.clone(), inst.automation.restart_warnings.clone());
            state.with_runtime(&inst.id, |r| r.restart_pending_at = Some(now_ms())).await;
            tauri::async_runtime::spawn(async move {
                notify(&app2, &id, "warn", format!("Restarting {name}: {why}"));
                if let Err(e) = server::countdown(&st, &app2, &id, "restart", &warn, &why).await {
                    notify(&app2, &id, "error", format!("Restart failed: {e}"));
                }
            });
        }
    }

    async fn tail_log(&mut self, state: &Arc<AppState>, app: &AppHandle, inst: &ServerInstance, facts: &mut Vec<Fact>) {
        let live = state.with_runtime(&inst.id, |r| r.state.map(|s| s.is_live()).unwrap_or(false)).await;
        if !live {
            return;
        }
        let path = inst.log_path();
        let Ok(meta) = std::fs::metadata(&path) else { return };
        let len = meta.len();
        let offset = state.with_runtime(&inst.id, |r| r.log_offset).await;
        let start = if len < offset { 0 } else { offset };
        if len == start {
            return;
        }
        let start = start.max(len.saturating_sub(2 * 1_048_576));
        let mut buf = vec![];
        let ok = std::fs::File::open(&path)
            .and_then(|mut f| {
                f.seek(SeekFrom::Start(start))?;
                f.take(len - start).read_to_end(&mut buf)
            })
            .is_ok();
        if !ok {
            return;
        }
        let text = String::from_utf8_lossy(&buf).into_owned();
        let mut fatal = None;
        let mut ready = false;
        let mut new_http = false;
        let lines = state
            .with_runtime(&inst.id, |r| {
                r.log_offset = len;
                r.last_log_at = now_ms();
                let mut all = std::mem::take(&mut r.log_partial);
                all.push_str(&text);
                let mut lines: Vec<String> = all.split('\n').map(|l| l.trim_end_matches('\r').to_string()).collect();
                r.log_partial = lines.pop().unwrap_or_default();
                let mut http_section: Option<String> = None;
                for l in &lines {
                    if r.engine_build.is_none() {
                        if let Some(c) = BUILD_RE.captures(l) {
                            r.engine_build = Some(c[1].to_string());
                        }
                    }
                    if let Some(c) = PROJECT_RE.captures(l) {
                        r.game_version = Some(c[1].to_string());
                    }
                    if let Some(c) = DEVKIT_RE.captures(l) {
                        r.devkit_revision = c[1].parse().ok();
                    }
                    if let Some(c) = MOD_LOADED_RE.captures(l) {
                        if !r.mods_loaded.iter().any(|m| m == &c[1]) {
                            r.mods_loaded.push(c[1].to_string());
                        }
                    }
                    if let Some(c) = CONTROLLER_RE.captures(l) {
                        r.merging_mod = (&c[1] == "Mods").then(|| c[2].to_string());
                    }
                    if let Some(c) = MERGE_RE.captures(l) {
                        if let Some(m) = r.merging_mod.clone() {
                            let v = r.row_overrides.entry(format!("{}:{}", &c[1], c[2].trim())).or_default();
                            if !v.contains(&m) {
                                v.push(m);
                            }
                        }
                    }
                    if let Some(c) = HTTP_SECTION_RE.captures(l) {
                        http_section = Some(c[1].to_string());
                    } else if let Some(c) = HTTP_ORIGIN_RE.captures(l) {
                        if let Some(sec) = &http_section {
                            let entry = (sec.clone(), c[1].to_string(), c.get(2).map(|m| m.as_str().to_string()).unwrap_or_default());
                            if !r.http_requests.contains(&entry) {
                                r.http_requests.push(entry);
                                new_http = true;
                            }
                        }
                    }
                    if r.state == Some(ServerState::Starting) && READY_RE.is_match(l) {
                        ready = true;
                    }
                    if fatal.is_none() && FATAL_RE.is_match(l) {
                        fatal = Some(l.clone());
                    }
                }
                lines
            })
            .await;
        if lines.is_empty() {
            return;
        }
        // Only patterns somebody asked for are checked against every line.
        if inst.automation.rules.iter().any(|r| r.enabled && matches!(r.trigger, Trigger::LogMatch { .. })) {
            for l in &lines {
                facts.push(Fact::Log(l.clone()));
            }
        }
        let shown: Vec<String> = if lines.len() > MAX_LINES_PER_TICK {
            let mut v = vec![format!("… {} earlier lines skipped (open the log file for everything)", lines.len() - MAX_LINES_PER_TICK)];
            v.extend(lines[lines.len() - MAX_LINES_PER_TICK..].iter().cloned());
            v
        } else {
            lines
        };
        let _ = app.emit("log-lines", LogBatch { id: inst.id.clone(), source: "server".into(), lines: shown, ts: now_ms() });
        if let Some(l) = fatal {
            notify(app, &inst.id, "error", format!("Server error: {}", l.trim().chars().take(300).collect::<String>()));
        }
        if new_http {
            notify(app, &inst.id, "warn", "A mod is asking for internet access – approve or deny it on the Mods page.");
        }
        if ready {
            state.with_runtime(&inst.id, |r| r.log_ready = true).await;
        }
    }

    /// RCON `listplayers` every `poll_secs` while running → join/leave events and sessions.
    async fn poll_players(&mut self, state: &Arc<AppState>, app: &AppHandle, inst: &ServerInstance, now: i64, facts: &mut Vec<Fact>) {
        if !inst.rcon.enabled || inst.rcon.poll_secs == 0 || inst.rcon.password.is_empty() {
            return;
        }
        let (running, last) = state.with_runtime(&inst.id, |r| (r.state == Some(ServerState::Running), r.last_player_poll)).await;
        if !running || now - last < inst.rcon.poll_secs as i64 * 1000 || *self.polling.get(&inst.id).unwrap_or(&false) {
            return;
        }
        state.with_runtime(&inst.id, |r| r.last_player_poll = now).await;
        self.polling.insert(inst.id.clone(), true);
        let res = state.rcon_exec(inst, "listplayers").await;
        self.polling.insert(inst.id.clone(), false);
        let Ok(text) = res else { return };
        let list = super::rcon::parse_listplayers(&text);
        let (joined, left) = state
            .with_runtime(&inst.id, |r| {
                let key = |p: &OnlinePlayer| if p.user_id.is_empty() { p.player_name.clone() } else { p.user_id.clone() };
                let joined: Vec<OnlinePlayer> = list.iter().filter(|p| !r.players.iter().any(|o| key(o) == key(p))).cloned().collect();
                let left: Vec<OnlinePlayer> = r.players.iter().filter(|o| !list.iter().any(|p| key(o) == key(p))).cloned().collect();
                r.players = list.clone();
                (joined, left)
            })
            .await;
        for p in joined {
            state.history.session_open(&inst.id, &p);
            let _ = app.emit("player-event", PlayerEvent { id: inst.id.clone(), player: p.clone(), kind: "join".into(), ts: now });
            notify(app, &inst.id, "info", format!("{} ({}) joined", p.player_name, p.char_name));
            facts.push(Fact::Join);
        }
        for p in left {
            state.history.session_close(&inst.id, &p.user_id);
            let _ = app.emit("player-event", PlayerEvent { id: inst.id.clone(), player: p.clone(), kind: "leave".into(), ts: now });
            notify(app, &inst.id, "info", format!("{} ({}) left", p.player_name, p.char_name));
            facts.push(Fact::Leave);
        }
    }

    /// Hung = running, no log output *and* no RCON answer for `hang_minutes`. Requires RCON so a
    /// quiet-but-healthy server is never killed.
    async fn hang_check(&mut self, state: &Arc<AppState>, app: &AppHandle, inst: &ServerInstance, now: i64) {
        let mins = inst.automation.hang_minutes;
        if mins == 0 || !inst.rcon.enabled {
            return;
        }
        let limit = mins as i64 * 60_000;
        let hung = state
            .with_runtime(&inst.id, |r| {
                r.state == Some(ServerState::Running)
                    && r.restart_pending_at.is_none()
                    && r.ready_at.map(|t| now - t > limit).unwrap_or(false)
                    && now - r.last_log_at > limit
                    && now - r.last_rcon_ok_at > limit
            })
            .await;
        if !hung {
            return;
        }
        // Confirm with one more RCON attempt before acting.
        if state.rcon_exec(inst, "listplayers").await.is_ok() {
            return;
        }
        state.with_runtime(&inst.id, |r| r.restart_pending_at = Some(now)).await;
        notify(app, &inst.id, "error", format!("{} looks frozen (no log output and no RCON answer for {mins} min) – restarting it.", inst.name));
        let (st, app2, id) = (state.clone(), app.clone(), inst.id.clone());
        tauri::async_runtime::spawn(async move {
            let _ = server::stop(&st, &app2, &id, true).await;
            if let Err(e) = server::start(&st, &app2, &id).await {
                notify(&app2, &id, "error", format!("Restart after freeze failed: {e}"));
            }
            st.with_runtime(&id, |r| r.restart_pending_at = None).await;
        });
    }

    async fn automation(&mut self, state: &Arc<AppState>, app: &AppHandle, inst: &ServerInstance, alive: bool, now: i64, facts: &mut Vec<Fact>) {
        let a = &inst.automation;
        let local = Local::now();

        // Schedules: fire on the matching minute, compute the next one for the UI.
        let mut next: Option<NextSchedule> = None;
        let mut fire = vec![];
        for (idx, e) in a.schedules.iter().enumerate().filter(|(_, e)| e.enabled) {
            let Ok(t) = NaiveTime::parse_from_str(e.time.trim(), "%H:%M") else { continue };
            let day_ok = |wd: u32| e.days.is_empty() || e.days.contains(&(wd as u8));
            if day_ok(local.weekday().num_days_from_sunday()) && local.hour() == t.hour() && local.minute() == t.minute() {
                let key = format!("{}-{idx}-{}-{}", local.format("%Y%m%d"), e.time, e.action);
                let fresh = state.with_runtime(&inst.id, |r| r.fired_schedule_keys.insert(key)).await;
                if fresh {
                    fire.push(e.clone());
                }
            }
            for d in 0..8 {
                let date = local.date_naive() + chrono::Days::new(d);
                let Some(dt) = Local.from_local_datetime(&date.and_time(t)).single() else { continue };
                if dt <= local || !day_ok(dt.weekday().num_days_from_sunday()) {
                    continue;
                }
                let at = dt.timestamp_millis();
                if next.as_ref().map(|n| at < n.at).unwrap_or(true) {
                    next = Some(NextSchedule { at, action: e.action.clone() });
                }
                break;
            }
        }
        state
            .with_runtime(&inst.id, |r| {
                r.next_schedule = next;
                if r.fired_schedule_keys.len() > 200 {
                    r.fired_schedule_keys.clear();
                }
            })
            .await;
        for e in fire {
            manager_log(app, &inst.id, format!("Scheduled {}", e.action));
            let action = match e.action.as_str() {
                "restart" => Action::Restart { warn_minutes: a.restart_warnings.clone() },
                "update" => Action::UpdateServer,
                "updateMods" => Action::UpdateMods,
                "stop" => Action::Stop,
                "start" => Action::Start,
                "backup" => Action::Backup { label: "Scheduled".into() },
                "broadcast" => Action::Broadcast { message: e.payload.clone() },
                "rcon" => Action::Rcon { command: e.payload.clone() },
                _ => continue,
            };
            run_actions(state.clone(), app.clone(), inst.id.clone(), vec![action], format!("schedule {}", e.time));
        }

        // Interval backups while running.
        if alive && a.backup_interval_mins > 0 {
            let due = state
                .with_runtime(&inst.id, |r| {
                    if r.last_backup == 0 {
                        r.last_backup = now;
                    }
                    let due = r.state == Some(ServerState::Running) && r.busy.is_none() && now - r.last_backup >= a.backup_interval_mins as i64 * 60_000;
                    if due {
                        r.last_backup = now;
                    }
                    due
                })
                .await;
            if due {
                let (st, app2, id) = (state.clone(), app.clone(), inst.id.clone());
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = run_backup(&st, &app2, &id, "Interval", "auto").await {
                        notify(&app2, &id, "warn", format!("Automatic backup failed: {e}"));
                    }
                });
            }
        }

        // Server update checks.
        if a.update_check_mins > 0 && inst.is_installed() {
            let due = state
                .with_runtime(&inst.id, |r| {
                    let due = now - r.last_update_check >= a.update_check_mins as i64 * 60_000 && r.busy.is_none();
                    if due {
                        r.last_update_check = now;
                    }
                    due
                })
                .await;
            if due {
                let (st, app2, id) = (state.clone(), app.clone(), inst.id.clone());
                tauri::async_runtime::spawn(async move {
                    if let Ok(true) = check_update(&st, &app2, &id, true).await {
                        fire_fact(&st, &app2, &id, Fact::UpdateAvailable).await;
                    }
                });
            }
        }

        // Mod update checks.
        if a.mod_update_check_mins > 0 && inst.mods.iter().any(|m| m.enabled && !m.workshop_id.is_empty()) {
            let due = state
                .with_runtime(&inst.id, |r| {
                    let due = now - r.last_mod_check >= a.mod_update_check_mins as i64 * 60_000;
                    if due {
                        r.last_mod_check = now;
                    }
                    due
                })
                .await;
            if due {
                let (st, app2, id) = (state.clone(), app.clone(), inst.id.clone());
                tauri::async_runtime::spawn(async move {
                    let _ = check_mod_updates(&st, &app2, &id, true).await;
                });
            }
        }
        let _ = facts;
    }

    async fn rules(&mut self, state: &Arc<AppState>, app: &AppHandle, inst: &ServerInstance, now: i64, facts: &[Fact]) {
        if inst.automation.rules.is_empty() {
            return;
        }
        let sample = self.last_sample.get(&inst.id).cloned();
        let (players, running) = state.with_runtime(&inst.id, |r| (r.players.len() as u32, r.state == Some(ServerState::Running))).await;
        for rule in inst.automation.rules.iter().filter(|r| r.enabled) {
            let hit = match &rule.trigger {
                Trigger::Interval { minutes } => {
                    let last = state.with_runtime(&inst.id, |r| *r.rule_fired.entry(rule.id.clone()).or_insert(now)).await;
                    *minutes > 0 && now - last >= *minutes as i64 * 60_000
                }
                Trigger::Crash => facts.contains(&Fact::Crash),
                Trigger::Started => facts.contains(&Fact::Started),
                Trigger::Stopped => facts.contains(&Fact::Stopped),
                Trigger::PlayerJoin => facts.contains(&Fact::Join),
                Trigger::PlayerLeave => facts.contains(&Fact::Leave),
                Trigger::UpdateAvailable => facts.contains(&Fact::UpdateAvailable),
                Trigger::ModUpdateAvailable => facts.contains(&Fact::ModUpdateAvailable),
                Trigger::PlayersAbove { count } => self.edge(state, inst, &rule.id, running && players > *count).await,
                Trigger::PlayersBelow { count } => self.edge(state, inst, &rule.id, running && players < *count).await,
                Trigger::MemoryAbove { mb } => self.edge(state, inst, &rule.id, sample.as_ref().map(|s| s.mem > *mb as u64 * 1_048_576).unwrap_or(false)).await,
                Trigger::CpuAbove { percent } => self.edge(state, inst, &rule.id, sample.as_ref().map(|s| s.cpu > *percent as f32).unwrap_or(false)).await,
                Trigger::LogMatch { pattern } => {
                    let re = self.regex_cache.entry(pattern.clone()).or_insert_with(|| Regex::new(pattern).ok()).clone();
                    match re {
                        Some(re) => facts.iter().any(|f| matches!(f, Fact::Log(l) if re.is_match(l))),
                        None => false,
                    }
                }
            };
            if !hit || !conditions_hold(&rule.conditions, players, running) {
                continue;
            }
            let ok = state
                .with_runtime(&inst.id, |r| {
                    let last = r.rule_fired.get(&rule.id).copied().unwrap_or(0);
                    let interval = matches!(rule.trigger, Trigger::Interval { .. });
                    if !interval && now - last < rule.cooldown_secs as i64 * 1000 {
                        return false;
                    }
                    r.rule_fired.insert(rule.id.clone(), now);
                    true
                })
                .await;
            if ok {
                manager_log(app, &inst.id, format!("Rule “{}” triggered", rule.name));
                state.history.event(Some(&inst.id), "info", "rule", &format!("Rule “{}” triggered", rule.name));
                run_actions(state.clone(), app.clone(), inst.id.clone(), rule.actions.clone(), format!("rule “{}”", rule.name));
            }
        }
    }

    /// True only on the tick a condition becomes true (so threshold rules fire once per crossing).
    async fn edge(&self, state: &Arc<AppState>, inst: &ServerInstance, rule_id: &str, now_true: bool) -> bool {
        state
            .with_runtime(&inst.id, |r| {
                let was = r.rule_edges.insert(rule_id.to_string(), now_true).unwrap_or(false);
                now_true && !was
            })
            .await
    }
}

fn parse_hm(s: &str) -> Option<u32> {
    let t = NaiveTime::parse_from_str(s.trim(), "%H:%M").ok()?;
    Some(t.hour() * 60 + t.minute())
}

fn conditions_hold(conds: &[Condition], players: u32, running: bool) -> bool {
    let local = Local::now();
    let mins = local.hour() * 60 + local.minute();
    conds.iter().all(|c| match c {
        Condition::PlayersAtMost { count } => players <= *count,
        Condition::PlayersAtLeast { count } => players >= *count,
        Condition::TimeBetween { from, to } => match (parse_hm(from), parse_hm(to)) {
            (Some(f), Some(t)) if f <= t => mins >= f && mins < t,
            (Some(f), Some(t)) => mins >= f || mins < t,
            _ => true,
        },
        Condition::Weekdays { days } => days.is_empty() || days.contains(&(local.weekday().num_days_from_sunday() as u8)),
        Condition::Running => running,
        Condition::NotRunning => !running,
    })
}

async fn fire_fact(state: &Arc<AppState>, app: &AppHandle, id: &str, fact: Fact) {
    // Facts produced outside the tick (async checks) run the matching rules directly.
    let Ok(inst) = state.instance(id).await else { return };
    let now = now_ms();
    for rule in inst.automation.rules.iter().filter(|r| r.enabled) {
        let hit = matches!((&rule.trigger, &fact), (Trigger::UpdateAvailable, Fact::UpdateAvailable) | (Trigger::ModUpdateAvailable, Fact::ModUpdateAvailable));
        if !hit {
            continue;
        }
        let (players, running) = state.with_runtime(id, |r| (r.players.len() as u32, r.state == Some(ServerState::Running))).await;
        if !conditions_hold(&rule.conditions, players, running) {
            continue;
        }
        let ok = state
            .with_runtime(id, |r| {
                let last = r.rule_fired.get(&rule.id).copied().unwrap_or(0);
                if now - last < rule.cooldown_secs as i64 * 1000 {
                    return false;
                }
                r.rule_fired.insert(rule.id.clone(), now);
                true
            })
            .await;
        if ok {
            run_actions(state.clone(), app.clone(), id.to_string(), rule.actions.clone(), format!("rule “{}”", rule.name));
        }
    }
}

/// Execute automation actions in order on a background task.
fn run_actions(state: Arc<AppState>, app: AppHandle, id: String, actions: Vec<Action>, why: String) {
    tauri::async_runtime::spawn(async move {
        for a in actions {
            let res: crate::error::AppResult<()> = async {
                match &a {
                    Action::Start => server::start(&state, &app, &id).await,
                    Action::Stop => {
                        let w = state.instance(&id).await?.automation.restart_warnings;
                        server::countdown(&state, &app, &id, "stop", &w, &why).await
                    }
                    Action::Restart { warn_minutes } => server::countdown(&state, &app, &id, "restart", warn_minutes, &why).await,
                    Action::Backup { label } => run_backup(&state, &app, &id, if label.is_empty() { "Automation" } else { label }, "auto").await,
                    Action::UpdateServer => {
                        let w = state.instance(&id).await?.automation.restart_warnings;
                        let inst = state.instance(&id).await?;
                        if server::running_pids(&inst).1.is_some() {
                            server::countdown(&state, &app, &id, "update", &w, &why).await
                        } else {
                            server::update_job(&state, &app, &id, false).await
                        }
                    }
                    Action::UpdateMods => update_mods_job(&state, &app, &id, &why).await,
                    Action::Rcon { command } => {
                        let inst = state.instance(&id).await?;
                        let out = state.rcon_exec(&inst, command).await?;
                        let _ = app.emit("log-lines", LogBatch { id: id.clone(), source: "rcon".into(), lines: vec![format!("> {command}"), out], ts: now_ms() });
                        Ok(())
                    }
                    Action::Broadcast { message } => {
                        let inst = state.instance(&id).await?;
                        state.rcon_exec(&inst, &format!("broadcast {message}")).await.map(|_| ())
                    }
                    Action::Notify { level, message } => {
                        notify(&app, &id, level, message.clone());
                        Ok(())
                    }
                    Action::Wait { seconds } => {
                        tokio::time::sleep(Duration::from_secs((*seconds).min(86_400) as u64)).await;
                        Ok(())
                    }
                }
            }
            .await;
            if let Err(e) = res {
                notify(&app, &id, "error", format!("Automation ({why}) failed: {e}"));
                break;
            }
        }
    });
}

/// Download mod updates; if the server runs, restart it (with warnings) to load them.
pub async fn update_mods_job(state: &Arc<AppState>, app: &AppHandle, id: &str, why: &str) -> crate::error::AppResult<()> {
    let inst = state.instance(id).await?;
    let report = modsync::sync(state, app, &inst, true, false).await?;
    if report.updated.is_empty() && report.downloaded.is_empty() {
        return Ok(());
    }
    if server::running_pids(&inst).1.is_some() {
        server::countdown(state, app, id, "restart", &inst.automation.restart_warnings, why).await?;
    }
    Ok(())
}

pub async fn check_mod_updates(state: &Arc<AppState>, app: &AppHandle, id: &str, background: bool) -> crate::error::AppResult<Vec<String>> {
    let inst = state.instance(id).await?;
    let list = modsync::outdated(state, &inst).await;
    let changed = state
        .with_runtime(id, |r| {
            let changed = r.mod_updates != list;
            r.mod_updates = list.clone();
            changed
        })
        .await;
    let _ = app.emit("mods-changed", id);
    if !list.is_empty() && changed && background {
        let names: Vec<String> = list.iter().map(|w| inst.mods.iter().find(|m| &m.workshop_id == w).map(|m| m.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| w.clone())).collect();
        notify(app, id, "info", format!("Mod update available: {}", names.join(", ")));
        fire_fact(state, app, id, Fact::ModUpdateAvailable).await;
        if inst.automation.auto_update_mods {
            let (st, app2, id2) = (state.clone(), app.clone(), id.to_string());
            tauri::async_runtime::spawn(async move {
                if let Err(e) = update_mods_job(&st, &app2, &id2, "mod update").await {
                    notify(&app2, &id2, "error", format!("Automatic mod update failed: {e}"));
                }
            });
        }
    }
    Ok(list)
}

#[derive(Default)]
struct LogMeta {
    build: Option<String>,
    project: Option<String>,
    devkit: Option<u32>,
    mods: Vec<String>,
}

/// Version, devkit and loaded mods from an existing log (when adopting a running server).
fn scan_log_meta(path: &std::path::Path) -> LogMeta {
    let text = std::fs::read(path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let mut m = LogMeta::default();
    for l in text.lines() {
        if m.build.is_none() {
            if let Some(c) = BUILD_RE.captures(l) {
                m.build = Some(c[1].to_string());
            }
        }
        if let Some(c) = PROJECT_RE.captures(l) {
            m.project = Some(c[1].to_string());
        }
        if let Some(c) = DEVKIT_RE.captures(l) {
            m.devkit = c[1].parse().ok();
        }
        if let Some(c) = MOD_LOADED_RE.captures(l) {
            if !m.mods.iter().any(|x| x == &c[1]) {
                m.mods.push(c[1].to_string());
            }
        }
    }
    m
}

pub async fn run_backup(state: &Arc<AppState>, app: &AppHandle, id: &str, label: &str, kind: &str) -> crate::error::AppResult<()> {
    let inst = state.instance(id).await?;
    state.set_busy(app, id, Some("Backing up")).await;
    let res = backup::create(state, &inst, label, kind).await;
    state.set_busy(app, id, None).await;
    let b = res?;
    state.with_runtime(id, |r| r.last_backup = b.created_at).await;
    if !b.verified && inst.db_path().exists() {
        notify(app, id, "warn", "Backup created, but the world database copy failed its integrity check – keep older backups!");
    }
    let _ = app.emit("backup-created", &b);
    Ok(())
}

/// Look up the live build for the server's branch (cached 10 min across servers) and, when the
/// profile asks for it, apply the update automatically. Returns true when an update is available.
pub async fn check_update(state: &Arc<AppState>, app: &AppHandle, id: &str, background: bool) -> crate::error::AppResult<bool> {
    let inst = state.instance(id).await?;
    let branch = if inst.branch.trim().is_empty() { "public".to_string() } else { inst.branch.trim().to_string() };
    let cached = state.latest_build_cache.lock().await.get(&branch).cloned().filter(|(t, _)| now_ms() - t < 10 * 60_000);
    let latest = match cached {
        Some((_, b)) => b,
        None => {
            let mut r = TaskReporter::new(app, "update-check", "Checking for server updates", Some(id));
            if background {
                r = r.quiet();
            }
            let res = match steamcmd::latest_build_web(&state.http, &branch).await {
                Some(b) => Ok(Some(b)),
                None => {
                    let steam = state.steamcmd_dir().await;
                    let _l = state.steam_lock.lock().await;
                    steamcmd::latest_build(&state.http, &steam, &branch, &r).await
                }
            };
            let b = match res {
                Ok(b) => b,
                Err(e) => {
                    if !background {
                        r.fail(&e.to_string());
                    }
                    return Err(e);
                }
            };
            if !background {
                r.done(&format!("Live build: {}", b.clone().unwrap_or_else(|| "unknown".into())));
            }
            state.latest_build_cache.lock().await.insert(branch, (now_ms(), b.clone()));
            b
        }
    };
    let installed = steamcmd::local_build(&inst.install_dir);
    state
        .with_runtime(id, |r| {
            r.latest_build = latest.clone();
            r.installed_build = installed.clone();
        })
        .await;
    state.emit_status(app, id).await;
    let available = matches!((&installed, &latest), (Some(i), Some(l)) if i != l);
    if available && background {
        if inst.automation.auto_update {
            notify(app, id, "info", format!("Update found for {} – applying it now", inst.name));
            let (st, app2, id2, w) = (state.clone(), app.clone(), id.to_string(), inst.automation.restart_warnings.clone());
            tauri::async_runtime::spawn(async move {
                let running = st.instance(&id2).await.map(|i| server::running_pids(&i).1.is_some()).unwrap_or(false);
                let res = if running { server::countdown(&st, &app2, &id2, "update", &w, "game update").await } else { server::update_job(&st, &app2, &id2, false).await };
                if let Err(e) = res {
                    notify(&app2, &id2, "error", format!("Automatic update failed: {e}"));
                }
            });
        } else {
            notify(app, id, "info", format!("A server update is available for {}", inst.name));
        }
    }
    Ok(available)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_enhanced_log_lines() {
        assert_eq!(&BUILD_RE.captures("LogInit: Build: ++exiles+release-CL-379176").unwrap()[1], "++exiles+release-CL-379176");
        assert!(BUILD_RE.captures("LogInit: Build Configuration: Shipping").is_none());
        assert_eq!(&PROJECT_RE.captures("[2026.10.10-07.06.18:624][  0]LogNetVersion: Set ProjectVersion to 3.0.1. Version Checksum will be recalculated on next use.").unwrap()[1], "3.0.1");
        assert_eq!(&DEVKIT_RE.captures("[2026.10.10-07.06.17:592][  0]LogModManager: SetCompatibleDevkitVersions: set 1 version(s) from 1 input string(s): [1002]").unwrap()[1], "1002");
        assert!(READY_RE.is_match("[2026.10.10-07.06.40:523][  0]LogServerStats: Startup report. StartupTime=24 Name=Conan Exiles Server Map=ConanSandbox QueryPort=27015 Type=2 Region=0 MaxPlayers=40"));
        assert!(READY_RE.is_match("[2026.10.10-07.06.40:524][  0]SourceServerQueries: Warning: Started SourceServerQueries on port 27015"));
        assert_eq!(&MOD_LOADED_RE.captures("[2026.10.10-07.12.50:886][  0]LogModManager: Mod 'Growth_Rates_100' contributes 5 package(s) to provenance map.").unwrap()[1], "Growth_Rates_100");
        let c = CONTROLLER_RE.captures("[2026.10.10-07.12.51:832][  0]LogModManager: AddActiveModControllerClass: /Game/Mods/Growth_Rates_100/GR_ModController.GR_ModController_C").unwrap();
        assert_eq!((&c[1], &c[2]), ("Mods", "Growth_Rates_100"));
        let d = CONTROLLER_RE.captures("LogModManager: AddActiveModControllerClass: /Game/DLC/DLC_Siptah/DLC_Siptah_Modcontroller.DLC_Siptah_Modcontroller_C").unwrap();
        assert_eq!(&d[1], "DLC");
        let m = MERGE_RE.captures("[2026.10.10-07.12.51:832][  0]LogModController: Display: AModController::MergeDataTables - Found existing table entry for /Game/Systems/Thrall/NPCStatsAndGrowthsDataTable.NPCStatsAndGrowthsDataTable with RowName: Bear_Alpha, overwriting.").unwrap();
        assert_eq!((&m[1], &m[2]), ("NPCStatsAndGrowthsDataTable", "Bear_Alpha"));
        let h = HTTP_SECTION_RE.captures(r#"LogModHttp: Warning: [HttpTest] ; mod "HTTP Demonstrator Mod""#).unwrap();
        assert_eq!(&h[1], "HttpTest");
        let o = HTTP_ORIGIN_RE.captures(r#"LogModHttp: Warning: AllowedOrigin="https://discord.com:443" ; reason: "post join/leave announcements""#).unwrap();
        assert_eq!((&o[1], &o[2]), ("https://discord.com:443", "post join/leave announcements"));
    }

    #[test]
    fn time_window_conditions() {
        assert!(conditions_hold(&[], 0, false));
        assert!(conditions_hold(&[Condition::PlayersAtMost { count: 0 }], 0, true));
        assert!(!conditions_hold(&[Condition::PlayersAtLeast { count: 5 }], 2, true));
        assert!(conditions_hold(&[Condition::TimeBetween { from: "00:00".into(), to: "23:59".into() }], 0, true) || Local::now().hour() == 23);
        assert!(!conditions_hold(&[Condition::Running], 0, false));
    }
}
