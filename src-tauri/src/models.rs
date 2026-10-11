//! Persisted and event types. Everything is camelCase on the wire and mirrored in `src/lib/types.ts`.
//!
//! CURRENT TARGET: CONAN EXILES: ENHANCED (UE5). Facts behind the constants below are verified
//! against a real install – see AGENTS.md §3.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// "Conan Exiles Dedicated Server" – anonymous SteamCMD app. Branch `public` = Enhanced (UE5),
/// `conan-exiles-legacy` = the old UE4 server.
pub const SERVER_APP_ID: u32 = 443030;
/// The game itself; Workshop mods live under this app id (anonymous download works).
pub const GAME_APP_ID: u32 = 440900;
pub const LEGACY_BRANCH: &str = "conan-exiles-legacy";
pub const PROJECT: &str = "ConanSandbox";
/// Every gameplay rule lives in this one section of ServerSettings.ini.
pub const SETTINGS_SECTION: &str = "ServerSettings";
/// Devkit revision the current server build accepts (`LogModManager: SetCompatibleDevkitVersions`).
/// Used only as a fallback when the server log hasn't told us yet.
pub const KNOWN_DEVKIT_REVISION: u32 = 1002;

// ------------------------------------------------------------------------------- app settings

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub steamcmd_dir: Option<PathBuf>,
    pub default_install_root: Option<PathBuf>,
    pub backup_dir: Option<PathBuf>,
    /// Shared Workshop download cache (one copy of each mod for all servers).
    pub workshop_dir: Option<PathBuf>,
    pub last_data_dir: Option<PathBuf>,
    /// Supervisor sample interval for live telemetry.
    pub telemetry_interval_ms: u64,
    pub confirm_destructive: bool,
    pub reduce_motion: bool,
    pub compact: bool,
    /// Masks IPs, passwords and player ids in the UI (for streaming / screenshots).
    pub streamer_mode: bool,
    pub check_updates_on_launch: bool,
    pub accent: String,
    /// Closing the window keeps the manager (and its automation) running in the tray.
    pub close_to_tray: bool,
    pub desktop_notifications: bool,
    /// Discord/Slack-compatible webhook for server events. Empty = off. Stored encrypted.
    pub webhook_url: String,
    /// Event levels sent to the webhook: success | error | warn | info
    pub webhook_levels: Vec<String>,
    /// Optional Steam Web API key – enables in-app Workshop search. Stored encrypted.
    pub steam_api_key: String,
    /// Days of 1-minute metrics kept in manager.db.
    pub metrics_retention_days: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            steamcmd_dir: None,
            default_install_root: None,
            backup_dir: None,
            workshop_dir: None,
            last_data_dir: None,
            telemetry_interval_ms: 1000,
            confirm_destructive: true,
            reduce_motion: false,
            compact: false,
            streamer_mode: false,
            check_updates_on_launch: true,
            accent: "bronze".into(),
            close_to_tray: true,
            desktop_notifications: true,
            webhook_url: String::new(),
            webhook_levels: vec!["success".into(), "error".into()],
            steam_api_key: String::new(),
            metrics_retention_days: 30,
        }
    }
}

// --------------------------------------------------------------------------- server profile

/// Values the manager owns and writes into the game's INI files on every start.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Identity {
    /// Engine.ini `[OnlineSubsystem] ServerName` – the name in the server browser.
    pub server_name: String,
    /// Join password (empty = public). Written to ServerSettings and Engine.ini.
    pub server_password: String,
    /// In-game admin password (`MakeMeAdmin`). ServerSettings.ini `AdminPassword`.
    pub admin_password: String,
    pub motd: String,
}

impl Default for Identity {
    fn default() -> Self {
        Self { server_name: "Conan Exiles Enhanced Server".into(), server_password: String::new(), admin_password: String::new(), motd: String::new() }
    }
}

/// Values written to DedicatedServer.ini – the per-server gameplay/identity settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct DedicatedSettings {
    /// 32-char hex Player ID of the server owner.
    pub owner_id: String,
    /// Name shown in the server browser.
    pub server_name: String,
    /// Name of the world created on first start.
    pub default_world_name: String,
    /// In-game admin password (MakeMeAdmin).
    pub admin_password: String,
    /// Join password (empty = public).
    pub world_password: String,
    /// Crossplay | PC | PlayStation | Xbox | Nintendo
    pub platform_policy: String,
    /// bAllowSendingCrashDumps
    pub allow_crash_dumps: bool,
}

impl Default for DedicatedSettings {
    fn default() -> Self {
        Self {
            owner_id: String::new(),
            server_name: String::new(),
            default_world_name: "MyWorld".into(),
            admin_password: String::new(),
            world_password: String::new(),
            platform_policy: "Crossplay".into(),
            allow_crash_dumps: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum LaunchValue {
    Flag(bool),
    Value(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct LaunchSettings {
    /// UDP game port. The server also binds port + 1 (pinger).
    pub port: u16,
    /// UDP Steam query port (server browser).
    pub query_port: u16,
    pub max_players: u32,
    /// Bind to one local IP (`-MULTIHOME=`). Empty = all interfaces.
    pub multihome: String,
    /// `-MULTIHOMEHTTP=` – required together with MULTIHOME or the server won't register.
    pub multihome_http: String,
    /// Ask the router to forward the UDP ports over UPnP on every start.
    pub upnp: bool,
    /// Keep the engine's console window visible (hidden by default).
    pub show_console: bool,
    /// normal | belowNormal | aboveNormal | high | idle
    pub priority: String,
    /// CPU cores the server may use. Empty = all cores.
    pub affinity: Vec<u32>,
    /// Catalog switches (`-log`, `-NoCrashDialog` …) keyed exactly as passed.
    pub options: BTreeMap<String, LaunchValue>,
    /// Free-form extra arguments appended last.
    pub extra_args: String,
}

impl Default for LaunchSettings {
    fn default() -> Self {
        let mut options = BTreeMap::new();
        options.insert("-log".into(), LaunchValue::Flag(true));
        options.insert("-NoCrashDialog".into(), LaunchValue::Flag(true));
        Self {
            port: 7777,
            query_port: 27015,
            max_players: 40,
            multihome: String::new(),
            multihome_http: String::new(),
            upnp: false,
            show_console: false,
            priority: "normal".into(),
            affinity: vec![],
            options,
            extra_args: String::new(),
        }
    }
}

/// Game.ini `[RconPlugin]`. The manager provisions RCON by default: it is the only clean way to
/// shut the server down (saves + checkpoints the DB), change settings live and list players.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RconSettings {
    pub enabled: bool,
    /// Generated on creation; stored encrypted.
    pub password: String,
    /// TCP. Never opened in the firewall by the manager.
    pub port: u16,
    pub max_karma: u32,
    /// Poll `listplayers` every N seconds while running (0 = off).
    pub poll_secs: u32,
}

impl Default for RconSettings {
    fn default() -> Self {
        Self { enabled: true, password: String::new(), port: 25575, max_karma: 120, poll_secs: 10 }
    }
}

/// Map regions (Enhanced merges Exiled Lands and Isle of Siptah into one map).
pub const REGIONS: &[(&str, &str)] = &[("ExiledLands", "Exiled Lands"), ("IsleOfSiptah", "Isle of Siptah")];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct RegionSettings {
    /// Engine.ini `[Regions] ExcludedRegions` – sublevels of these regions aren't loaded (less RAM).
    pub excluded: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ModEntry {
    /// Steam Workshop id, or empty for a local pak.
    pub workshop_id: String,
    /// Absolute path of a local (non-Workshop) pak.
    pub local_path: Option<PathBuf>,
    pub enabled: bool,
    /// Cached display name (Workshop title or modinfo name).
    pub name: String,
}

impl Default for ModEntry {
    fn default() -> Self {
        Self { workshop_id: String::new(), local_path: None, enabled: true, name: String::new() }
    }
}

impl ModEntry {
    pub fn key(&self) -> String {
        if self.workshop_id.is_empty() {
            self.local_path.as_ref().map(|p| format!("local:{}", p.display())).unwrap_or_default()
        } else {
            self.workshop_id.clone()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ModProfile {
    pub id: String,
    pub name: String,
    pub mods: Vec<ModEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ScheduleEntry {
    pub enabled: bool,
    /// Local time, "HH:MM".
    pub time: String,
    /// 0 = Sunday … 6 = Saturday. Empty = every day.
    pub days: Vec<u8>,
    /// restart | backup | update | updateMods | stop | start | broadcast | rcon
    pub action: String,
    /// Message (broadcast) or command (rcon).
    pub payload: String,
}

impl Default for ScheduleEntry {
    fn default() -> Self {
        Self { enabled: true, time: "04:00".into(), days: vec![], action: "restart".into(), payload: String::new() }
    }
}

// Automation rules: trigger → conditions → actions.

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Trigger {
    Interval { minutes: u32 },
    Crash,
    Started,
    Stopped,
    PlayerJoin,
    PlayerLeave,
    PlayersAbove { count: u32 },
    PlayersBelow { count: u32 },
    UpdateAvailable,
    ModUpdateAvailable,
    MemoryAbove { mb: u32 },
    CpuAbove { percent: u32 },
    LogMatch { pattern: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Condition {
    PlayersAtMost { count: u32 },
    PlayersAtLeast { count: u32 },
    /// Local time window "HH:MM"–"HH:MM" (may wrap midnight).
    TimeBetween { from: String, to: String },
    Weekdays { days: Vec<u8> },
    Running,
    NotRunning,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Action {
    Start,
    Stop,
    /// Restart with in-game countdown broadcasts at these minutes before.
    Restart { warn_minutes: Vec<u32> },
    Backup { label: String },
    UpdateServer,
    UpdateMods,
    Rcon { command: String },
    Broadcast { message: String },
    Notify { level: String, message: String },
    Wait { seconds: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub trigger: Trigger,
    pub conditions: Vec<Condition>,
    pub actions: Vec<Action>,
    /// Minimum seconds between two firings.
    pub cooldown_secs: u32,
}

impl Default for Rule {
    fn default() -> Self {
        Self { id: String::new(), name: "New rule".into(), enabled: true, trigger: Trigger::Crash, conditions: vec![], actions: vec![], cooldown_secs: 300 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AutomationSettings {
    pub auto_start: bool,
    pub auto_restart: bool,
    pub restart_delay_secs: u32,
    /// Crash-loop guard: stop auto-restarting after this many crashes within 15 minutes.
    pub crash_limit: u32,
    pub stop_timeout_secs: u32,
    /// Minutes before a scheduled/automatic restart at which players are warned in-game.
    pub restart_warnings: Vec<u32>,
    /// `{minutes}` is replaced. Sent with RCON `broadcast`.
    pub restart_message: String,
    pub schedules: Vec<ScheduleEntry>,
    pub rules: Vec<Rule>,
    pub backup_interval_mins: u32,
    pub backup_keep: u32,
    pub backup_on_stop: bool,
    pub backup_before_update: bool,
    pub backup_include_mods: bool,
    pub update_on_start: bool,
    pub update_check_mins: u32,
    pub auto_update: bool,
    /// Check Workshop for mod updates every N minutes (0 = off).
    pub mod_update_check_mins: u32,
    /// Download mod updates automatically and restart (with warnings) when players allow.
    pub auto_update_mods: bool,
    /// Update mods on every start.
    pub update_mods_on_start: bool,
    /// Restart when the server's memory exceeds this many MB (0 = off).
    pub memory_limit_mb: u32,
    /// Restart when the server has been up this many hours (0 = off).
    pub max_uptime_hours: u32,
    /// Treat the server as hung after this many minutes without log output *and* without an
    /// RCON answer (0 = off).
    pub hang_minutes: u32,
}

impl Default for AutomationSettings {
    fn default() -> Self {
        Self {
            auto_start: false,
            auto_restart: true,
            restart_delay_secs: 10,
            crash_limit: 3,
            // Verified: a clean RCON shutdown of a lived-in world can take ~2.5 min before exit.
            stop_timeout_secs: 300,
            restart_warnings: vec![15, 5, 1],
            restart_message: "Server restart in {minutes} minute(s). Find a safe place and log out.".into(),
            schedules: vec![],
            rules: vec![],
            backup_interval_mins: 60,
            backup_keep: 48,
            backup_on_stop: true,
            backup_before_update: true,
            backup_include_mods: false,
            update_on_start: false,
            update_check_mins: 60,
            auto_update: false,
            mod_update_check_mins: 60,
            auto_update_mods: false,
            update_mods_on_start: true,
            memory_limit_mb: 0,
            max_uptime_hours: 0,
            hang_minutes: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerInstance {
    pub id: String,
    pub name: String,
    pub color: String,
    /// Lucide icon name for the sidebar/cards.
    pub icon: String,
    pub install_dir: PathBuf,
    pub created_at: i64,
    pub notes: String,
    pub tags: Vec<String>,
    pub folder_id: Option<String>,
    pub favorite: bool,
    pub archived: bool,
    /// Steam branch ("public" = Enhanced, "conan-exiles-legacy" = UE4, or a beta) and its password.
    pub branch: String,
    pub branch_password: String,
    pub identity: Identity,
    pub ini: DedicatedSettings,
    pub launch: LaunchSettings,
    pub rcon: RconSettings,
    pub regions: RegionSettings,
    /// Load order = vector order.
    pub mods: Vec<ModEntry>,
    pub mod_profiles: Vec<ModProfile>,
    pub automation: AutomationSettings,
}

impl Default for ServerInstance {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "Conan Exiles Server".into(),
            color: "bronze".into(),
            icon: "swords".into(),
            install_dir: PathBuf::new(),
            created_at: 0,
            notes: String::new(),
            tags: vec![],
            folder_id: None,
            favorite: false,
            archived: false,
            branch: "public".into(),
            branch_password: String::new(),
            identity: Identity::default(),
            ini: DedicatedSettings::default(),
            launch: LaunchSettings::default(),
            rcon: RconSettings::default(),
            regions: RegionSettings::default(),
            mods: vec![],
            mod_profiles: vec![],
            automation: AutomationSettings::default(),
        }
    }
}

impl ServerInstance {
    /// UDP game port + the pinger the engine always binds at game + 1.
    pub fn pinger_port(&self) -> u16 {
        self.launch.port.saturating_add(1)
    }

    /// Every UDP port the server binds and players/browsers need (game, pinger, query).
    pub fn udp_ports(&self) -> Vec<u16> {
        vec![self.launch.port, self.pinger_port(), self.launch.query_port]
    }

    pub fn is_legacy(&self) -> bool {
        self.branch.trim().eq_ignore_ascii_case(LEGACY_BRANCH)
    }

    pub fn project_dir(&self) -> PathBuf {
        self.install_dir.join(PROJECT)
    }
    pub fn saved_dir(&self) -> PathBuf {
        self.project_dir().join("Saved")
    }
    pub fn config_dir(&self) -> PathBuf {
        self.saved_dir().join("Config").join("WindowsServer")
    }
    pub fn settings_path(&self) -> PathBuf {
        self.config_dir().join("ServerSettings.ini")
    }
    pub fn dedicated_ini(&self) -> PathBuf {
        self.config_dir().join("DedicatedServer.ini")
    }
    pub fn engine_ini(&self) -> PathBuf {
        self.config_dir().join("Engine.ini")
    }
    pub fn game_ini(&self) -> PathBuf {
        self.config_dir().join("Game.ini")
    }
    pub fn modhttp_ini(&self) -> PathBuf {
        self.config_dir().join("ModHttp.ini")
    }
    /// The live world database. Enhanced uses `game_0.db` (save slots); `game.db` is the legacy
    /// name the server copies from on first start.
    pub fn db_path(&self) -> PathBuf {
        let slot = self.saved_dir().join("game_0.db");
        let legacy = self.saved_dir().join("game.db");
        if !slot.exists() && legacy.exists() && self.is_legacy() {
            legacy
        } else {
            slot
        }
    }
    pub fn logs_dir(&self) -> PathBuf {
        self.saved_dir().join("Logs")
    }
    pub fn log_path(&self) -> PathBuf {
        self.logs_dir().join(format!("{PROJECT}.log"))
    }
    pub fn mods_dir(&self) -> PathBuf {
        self.project_dir().join("Mods")
    }
    pub fn modlist_path(&self) -> PathBuf {
        self.mods_dir().join("modlist.txt")
    }

    /// The actual engine binary. Launching it directly (rather than the `ConanSandboxServer.exe`
    /// launcher in the root folder) keeps the PID we track equal to the process doing the work.
    pub fn server_exe(&self) -> PathBuf {
        resolve_server_exe(&self.install_dir)
    }

    pub fn is_installed(&self) -> bool {
        self.server_exe().exists()
    }
}

pub fn resolve_server_exe(install_dir: &Path) -> PathBuf {
    let bin = install_dir.join(PROJECT).join("Binaries").join("Win64");
    let shipping = bin.join("ConanSandboxServer-Win64-Shipping.exe");
    if shipping.exists() {
        return shipping;
    }
    if let Ok(rd) = std::fs::read_dir(&bin) {
        let mut found: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                n.ends_with(".exe") && n.starts_with("conansandboxserver")
            })
            .collect();
        found.sort();
        if let Some(p) = found.into_iter().next() {
            return p;
        }
    }
    shipping
}

// -------------------------------------------------------------------------------- runtime/status

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ServerState {
    NotInstalled,
    Stopped,
    Installing,
    Updating,
    Starting,
    Running,
    Stopping,
    Restarting,
    Crashed,
}

impl ServerState {
    pub fn is_live(self) -> bool {
        matches!(self, ServerState::Starting | ServerState::Running | ServerState::Stopping)
    }
}

/// One row of RCON `listplayers`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "camelCase")]
pub struct OnlinePlayer {
    pub idx: u32,
    pub char_name: String,
    pub player_name: String,
    pub user_id: String,
    pub platform_id: String,
    pub platform: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    pub id: String,
    pub state: ServerState,
    pub pid: Option<u32>,
    pub started_at: Option<i64>,
    pub ready_at: Option<i64>,
    pub installed_build: Option<String>,
    pub latest_build: Option<String>,
    /// e.g. `++exiles+release-CL-379176`
    pub engine_build: Option<String>,
    /// e.g. `3.0.1` (LogNetVersion ProjectVersion)
    pub game_version: Option<String>,
    pub crash_count: u32,
    pub crash_loop: bool,
    pub pending_restart: bool,
    pub players: Vec<OnlinePlayer>,
    /// The RCON connection answered recently.
    pub rcon_ok: bool,
    pub next_schedule: Option<NextSchedule>,
    pub last_backup: Option<i64>,
    pub last_exit_code: Option<i64>,
    pub busy: Option<String>,
    /// Mods the server mounted this session (folder names, in load order).
    pub mods_loaded: Vec<String>,
    /// Devkit revision the running build accepts.
    pub devkit_revision: Option<u32>,
    /// A restart countdown in progress (ms timestamp of the restart).
    pub restart_pending_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextSchedule {
    pub at: i64,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProcSample {
    pub id: String,
    pub cpu: f32,
    pub mem: u64,
    pub virt: u64,
    pub read_bps: u64,
    pub write_bps: u64,
    pub uptime: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SystemSample {
    pub cpu: f32,
    pub mem_used: u64,
    pub mem_total: u64,
    pub rx_bps: u64,
    pub tx_bps: u64,
    pub cores: usize,
    pub per_core: Vec<f32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryEvent {
    pub ts: i64,
    pub system: SystemSample,
    pub servers: Vec<ProcSample>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskEvent {
    pub task_id: String,
    pub instance_id: Option<String>,
    pub kind: String,
    pub title: String,
    pub message: String,
    pub progress: Option<f64>,
    pub done: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogBatch {
    pub id: String,
    /// server | manager | steamcmd | rcon
    pub source: String,
    pub lines: Vec<String>,
    pub ts: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerEvent {
    pub id: String,
    /// info | success | warn | error
    pub level: String,
    pub message: String,
    pub ts: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerEvent {
    pub id: String,
    pub player: OnlinePlayer,
    /// join | leave
    pub kind: String,
    pub ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub created_at: i64,
    pub label: String,
    pub kind: String,
    pub files: u32,
    /// The world database inside passed `PRAGMA integrity_check` when the backup was made.
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub includes_mods: bool,
    #[serde(default)]
    pub build: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    pub path: PathBuf,
    pub exists: bool,
    pub text: String,
    pub modified: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackManifest {
    pub format: u32,
    pub app_version: String,
    pub exported_at: i64,
    pub instance: ServerInstance,
    pub has_saves: bool,
    pub has_configs: bool,
    pub includes_secrets: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagCheck {
    pub id: String,
    pub title: String,
    /// ok | warn | error | info
    pub level: String,
    pub detail: String,
    /// Fix the UI can trigger: firewall | install | validate | ports | buildOverride | rcon | prereqs | mods | ""
    pub fix: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub data_dir: PathBuf,
    pub portable: bool,
    pub server_app_id: u32,
    pub game_app_id: u32,
    pub cores: usize,
    pub total_memory: u64,
    pub os: String,
    pub hostname: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListEntry {
    pub list: String,
    pub raw: String,
    pub fields: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveFile {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub modified: i64,
    pub active: bool,
}
