// Mirrors src-tauri/src/models.rs and the command payloads (serde camelCase).

export type LaunchValue = boolean | string;

export interface AppSettings {
  steamcmdDir: string | null;
  defaultInstallRoot: string | null;
  backupDir: string | null;
  workshopDir: string | null;
  lastDataDir?: string | null;
  telemetryIntervalMs: number;
  confirmDestructive: boolean;
  reduceMotion: boolean;
  compact: boolean;
  streamerMode: boolean;
  checkUpdatesOnLaunch: boolean;
  accent: string;
  closeToTray: boolean;
  desktopNotifications: boolean;
  webhookUrl: string;
  webhookLevels: string[];
  steamApiKey: string;
  metricsRetentionDays: number;
}

export interface Identity {
  serverName: string;
  serverPassword: string;
  adminPassword: string;
  motd: string;
}

export interface LaunchSettings {
  port: number;
  queryPort: number;
  beaconPort: number;
  maxPlayers: number;
  multihome: string;
  multihomeHttp: string;
  upnp: boolean;
  showConsole: boolean;
  priority: 'idle' | 'belowNormal' | 'normal' | 'aboveNormal' | 'high' | string;
  affinity: number[];
  options: Record<string, LaunchValue>;
  extraArgs: string;
}

export interface RconSettings {
  enabled: boolean;
  password: string;
  port: number;
  maxKarma: number;
  pollSecs: number;
}

export interface RegionSettings {
  excluded: string[];
}

export interface ModEntry {
  workshopId: string;
  localPath: string | null;
  enabled: boolean;
  name: string;
}

export interface ModProfile {
  id: string;
  name: string;
  mods: ModEntry[];
}

export type ScheduleAction = 'restart' | 'backup' | 'update' | 'updateMods' | 'stop' | 'start' | 'broadcast' | 'rcon';

export interface ScheduleEntry {
  enabled: boolean;
  time: string;
  days: number[];
  action: ScheduleAction | string;
  payload: string;
}

export type Trigger =
  | { kind: 'interval'; minutes: number }
  | { kind: 'crash' }
  | { kind: 'started' }
  | { kind: 'stopped' }
  | { kind: 'playerJoin' }
  | { kind: 'playerLeave' }
  | { kind: 'playersAbove'; count: number }
  | { kind: 'playersBelow'; count: number }
  | { kind: 'updateAvailable' }
  | { kind: 'modUpdateAvailable' }
  | { kind: 'memoryAbove'; mb: number }
  | { kind: 'cpuAbove'; percent: number }
  | { kind: 'logMatch'; pattern: string };

export type Condition =
  | { kind: 'playersAtMost'; count: number }
  | { kind: 'playersAtLeast'; count: number }
  | { kind: 'timeBetween'; from: string; to: string }
  | { kind: 'weekdays'; days: number[] }
  | { kind: 'running' }
  | { kind: 'notRunning' };

export type Action =
  | { kind: 'start' }
  | { kind: 'stop' }
  | { kind: 'restart'; warnMinutes: number[] }
  | { kind: 'backup'; label: string }
  | { kind: 'updateServer' }
  | { kind: 'updateMods' }
  | { kind: 'rcon'; command: string }
  | { kind: 'broadcast'; message: string }
  | { kind: 'notify'; level: string; message: string }
  | { kind: 'wait'; seconds: number };

export interface Rule {
  id: string;
  name: string;
  enabled: boolean;
  trigger: Trigger;
  conditions: Condition[];
  actions: Action[];
  cooldownSecs: number;
}

export interface AutomationSettings {
  autoStart: boolean;
  autoRestart: boolean;
  restartDelaySecs: number;
  crashLimit: number;
  stopTimeoutSecs: number;
  restartWarnings: number[];
  restartMessage: string;
  schedules: ScheduleEntry[];
  rules: Rule[];
  backupIntervalMins: number;
  backupKeep: number;
  backupOnStop: boolean;
  backupBeforeUpdate: boolean;
  backupIncludeMods: boolean;
  updateOnStart: boolean;
  updateCheckMins: number;
  autoUpdate: boolean;
  modUpdateCheckMins: number;
  autoUpdateMods: boolean;
  updateModsOnStart: boolean;
  memoryLimitMb: number;
  maxUptimeHours: number;
  hangMinutes: number;
}

export interface DedicatedSettings {
  ownerId: string;
  serverName: string;
  defaultWorldName: string;
  /** Map the server runs (Engine.ini `ServerDefaultMap`). Empty = the game's default map. */
  defaultMap: string;
  adminPassword: string;
  worldPassword: string;
  platformPolicy: string;
  allowCrashDumps: boolean;
}

export interface ServerInstance {
  id: string;
  name: string;
  color: string;
  icon: string;
  installDir: string;
  createdAt: number;
  notes: string;
  tags: string[];
  folderId: string | null;
  favorite: boolean;
  archived: boolean;
  branch: string;
  branchPassword: string;
  identity: Identity;
  ini: DedicatedSettings;
  launch: LaunchSettings;
  rcon: RconSettings;
  regions: RegionSettings;
  mods: ModEntry[];
  modProfiles: ModProfile[];
  automation: AutomationSettings;
}

export type ServerState =
  | 'notInstalled'
  | 'stopped'
  | 'installing'
  | 'updating'
  | 'starting'
  | 'running'
  | 'stopping'
  | 'restarting'
  | 'crashed';

export interface OnlinePlayer {
  idx: number;
  charName: string;
  playerName: string;
  userId: string;
  platformId: string;
  platform: string;
}

export interface ServerSession {
  SlotName: string;
  JoinCode: string;
  BuildId: string;
}

export interface ServerStatus {
  id: string;
  state: ServerState;
  pid: number | null;
  startedAt: number | null;
  readyAt: number | null;
  installedBuild: string | null;
  latestBuild: string | null;
  engineBuild: string | null;
  engineVersion: string | null;
  gameVersion: string | null;
  crashCount: number;
  crashLoop: boolean;
  pendingRestart: boolean;
  players: OnlinePlayer[];
  rconOk: boolean;
  session: ServerSession | null;
  nextSchedule: { at: number; action: string } | null;
  lastBackup: number | null;
  lastExitCode: number | null;
  busy: string | null;
  modsLoaded: string[];
  modded: boolean | null;
  devkitRevision: number | null;
  restartPendingAt: number | null;
}

export interface ProcSample {
  id: string;
  cpu: number;
  mem: number;
  virt: number;
  readBps: number;
  writeBps: number;
  uptime: number;
}

export interface SystemSample {
  cpu: number;
  memUsed: number;
  memTotal: number;
  rxBps: number;
  txBps: number;
  cores: number;
  perCore: number[];
}

export interface TelemetryEvent {
  ts: number;
  system: SystemSample;
  servers: ProcSample[];
}

export interface TaskEvent {
  taskId: string;
  instanceId: string | null;
  kind: string;
  title: string;
  message: string;
  progress: number | null;
  done: boolean;
  error: string | null;
}

export interface LogBatch {
  id: string;
  source: 'server' | 'manager' | 'steamcmd' | 'rcon' | string;
  lines: string[];
  ts: number;
}

export interface ServerEvent {
  id: string;
  level: 'info' | 'success' | 'warn' | 'error';
  message: string;
  ts: number;
}

export interface PlayerEvent {
  id: string;
  player: OnlinePlayer;
  kind: 'join' | 'leave';
  ts: number;
}

export interface BackupInfo {
  path: string;
  name: string;
  size: number;
  createdAt: number;
  label: string;
  kind: string;
  files: number;
  verified: boolean;
  includesMods: boolean;
  build: string | null;
}

export interface VerifyReport {
  ok: boolean;
  checked: number;
  mismatched: string[];
  missing: string[];
  dbOk: boolean | null;
}

export interface ConfigFile {
  path: string;
  exists: boolean;
  text: string;
  modified: number | null;
}

export interface PackManifest {
  format: number;
  appVersion: string;
  exportedAt: number;
  instance: ServerInstance;
  hasSaves: boolean;
  hasConfigs: boolean;
  includesSecrets: boolean;
}

export interface DiagCheck {
  id: string;
  title: string;
  level: 'ok' | 'warn' | 'error' | 'info';
  detail: string;
  fix: '' | 'firewall' | 'install' | 'validate' | 'settings' | 'ports' | 'prereqs' | 'update' | 'log' | 'rcon' | 'mods' | 'buildOverride' | string;
}

export interface DiagReport {
  checks: DiagCheck[];
  score: number;
}

export interface AppInfo {
  version: string;
  dataDir: string;
  portable: boolean;
  appId: number;
  serverAppId: number;
  gameAppId: number;
  cores: number;
  totalMemory: number;
  os: string;
  hostname: string;
}

export interface FolderProbe {
  exists: boolean;
  writable: boolean;
  empty: boolean;
  hasServer: boolean;
  freeBytes: number | null;
}

export interface Folder {
  id: string;
  name: string;
  collapsed: boolean;
}

export interface PortSet {
  game: number;
  query: number;
  rcon: number;
}

export interface FirewallState {
  ruleExists: boolean;
  portsOk: boolean;
  rulePorts: string;
  programRule: boolean;
  blockedBy: string[];
}

export interface NetInfo {
  publicIp: string | null;
  local: { interface: string; ip: string }[];
  gateway: string | null;
}

export interface UpnpMapping {
  externalPort: number;
  internalPort: number;
  internalClient: string;
  description: string;
  enabled: boolean;
}

export interface UpnpInfo {
  found: boolean;
  gateway: string | null;
  routerName: string | null;
  manufacturer: string | null;
  model: string | null;
  externalIp: string | null;
  wanKind: 'public' | 'private' | 'cgnat' | 'unknown';
  mappings: UpnpMapping[];
  error: string | null;
}

export interface LogFile {
  path: string;
  name: string;
  size: number;
  modified: number;
}

export interface DiskUsage {
  install: number;
  world: number;
  saves: number;
  backups: number;
  logs: number;
  mods: number;
}

export type ConfigFileName = 'ServerSettings.ini' | 'Engine.ini' | 'Game.ini' | 'ModHttp.ini' | 'DedicatedServer.ini' | 'GameUserSettings.ini';

export interface GameRules {
  exists: boolean;
  values: Record<string, string>;
  pending: Record<string, string>;
  configVersion: string | null;
  blueprintConfigVersion: string | null;
}

export interface ApplyResult {
  live: string[];
  written: string[];
  queued: string[];
  failed: [string, string][];
  restartNeeded: boolean;
}

// --- mods ---

export interface HttpOrigin {
  origin: string;
  reason: string;
}

export interface ModInfo {
  name: string;
  description: string;
  changeNote: string;
  author: string;
  authorUrl: string;
  version: string;
  folderName: string;
  devkitRevision: number | null;
  minimumVersion: string;
  workshopId: string;
  requestedHttpOrigins: HttpOrigin[];
}

export interface WorkshopItem {
  id: string;
  found: boolean;
  title: string;
  previewUrl: string;
  fileSize: number;
  timeCreated: number;
  timeUpdated: number;
  tags: string[];
  subscriptions: number;
  creator: string;
  banned: boolean;
  forConan: boolean;
  description: string;
}

export interface ModIssue {
  level: 'error' | 'warn' | 'info';
  message: string;
}

export interface ModView {
  entry: ModEntry;
  key: string;
  pak: string | null;
  size: number;
  info: ModInfo | null;
  workshop: WorkshopItem | null;
  localUpdated: number | null;
  updateAvailable: boolean;
  loaded: boolean;
  complete: boolean;
  issues: ModIssue[];
  health: number;
}

export interface ModConflict {
  row: string;
  mods: string[];
}

export interface HttpRequestView {
  modFolder: string;
  origin: string;
  reason: string;
  state: 'allowed' | 'denied' | 'pending';
}

export interface ModsView {
  mods: ModView[];
  serverDevkit: number;
  conflicts: ModConflict[];
  http: HttpRequestView[];
  workshopDir: string;
  folderExists: boolean;
  modsPath: string;
}

export type ModsState = ModsView;

export interface AddModsResult {
  added: string[];
  skipped: string[];
  missingDependencies: string[];
}

export interface ArchivedVersion {
  stamp: number;
  path: string;
  size: number;
}

// --- players / world ---

export interface PlayerSummary {
  userId: string;
  platformId: string;
  playerName: string;
  charName: string;
  sessions: number;
  playtimeSecs: number;
  firstSeen: number;
  lastSeen: number;
  online: boolean;
  note: string;
  warnings: number;
  kicks: number;
  bans: number;
}

export interface ModerationRow {
  id: number;
  userId: string;
  playerName: string;
  action: string;
  reason: string;
  actor: string;
  ts: number;
}

export interface Character {
  id: number;
  name: string;
  level: number;
  clanId: number | null;
  clan: string;
  alive: boolean;
  killer: string;
  lastOnline: number;
  accountId: string;
  user: string;
  platformId: string;
  online: boolean;
}

export interface Clan {
  id: number;
  name: string;
  motd: string;
  ownerId: number;
  owner: string;
  members: number;
  maxLevel: number;
  lastActive: number;
}

export interface GameEvent {
  time: number;
  eventType: number;
  object: string;
  owner: string;
  ownerClan: string;
  causer: string;
  causerClan: string;
  x: number;
  y: number;
  details: string;
}

export interface WorldSummary {
  sizeBytes: number;
  characters: number;
  alive: number;
  accounts: number;
  clans: number;
  buildings: number;
  buildingPieces: number;
  placeables: number;
  thrallsAndPets: number;
  dbVersion: string;
  serverRuntimeSecs: number;
}

/** A map discovered by scanning the installed game's `Content` folder for `.umap` assets. */
export interface MapInfo {
  /** Internal map path for `ServerDefaultMap` (e.g. `/Game/Maps/ConanSandbox/ConanSandbox`). */
  path: string;
  /** Human-friendly name derived from the asset file name. */
  label: string;
}

export interface SqlTable {
  columns: string[];
  rows: string[][];
}

export interface ListEntry {
  list: string;
  raw: string;
  fields: [string, string][];
}

export type PlatformPolicy = 'Crossplay' | 'PC' | 'PlayStation' | 'Xbox' | 'Nintendo' | string;

export interface SaveFile {
  name: string;
  path: string;
  size: number;
  modified: number;
  active: boolean;
}

// --- history ---

export interface MetricRow {
  ts: number;
  cpu: number;
  mem: number;
  readBps: number;
  writeBps: number;
  players: number;
  dbSize: number;
}

export interface EventRow {
  id: number;
  serverId: string | null;
  ts: number;
  level: string;
  kind: string;
  message: string;
}

export interface AuditRow {
  id: number;
  ts: number;
  actor: string;
  serverId: string | null;
  action: string;
  detail: string;
}
