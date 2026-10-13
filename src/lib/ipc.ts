import { invoke as tauriInvoke, isTauri } from '@tauri-apps/api/core';
import { listen as tauriListen } from '@tauri-apps/api/event';
import type * as T from './types';

type Handler = (args: Record<string, unknown>) => unknown | Promise<unknown>;
type Mock = { handlers: Record<string, Handler>; listeners: Map<string, Set<(p: unknown) => void>> };

let mock: Mock | null = null;

/** Browser preview only (`npm run dev` outside Tauri) – see devMock.ts. */
export function installMock(handlers: Record<string, Handler>) {
  mock = { handlers, listeners: new Map() };
}
export function mockEmit(event: string, payload: unknown) {
  mock?.listeners.get(event)?.forEach((fn) => fn(payload));
}

async function invoke<R>(cmd: string, args: Record<string, unknown> = {}): Promise<R> {
  if (mock) {
    const h = mock.handlers[cmd];
    if (!h) throw `Mock: command ${cmd} not implemented`;
    await new Promise((r) => setTimeout(r, 40));
    return (await h(args)) as R;
  }
  return tauriInvoke<R>(cmd, args);
}

export async function listen<P>(event: string, fn: (payload: P) => void): Promise<() => void> {
  if (mock) {
    const set = mock.listeners.get(event) ?? new Set();
    mock.listeners.set(event, set);
    const cb = (p: unknown) => fn(p as P);
    set.add(cb);
    return () => set.delete(cb);
  }
  return tauriListen<P>(event, (e) => fn(e.payload));
}

export const inTauri = () => isTauri();

export interface CreateInput {
  name: string;
  installDir?: string | null;
  color?: string;
  identity: T.Identity;
  ini?: T.DedicatedSettings;
  port?: number | null;
  queryPort?: number | null;
  rconPort?: number | null;
  maxPlayers?: number;
  branch?: string;
  installNow: boolean;
  templateId?: string | null;
  rules?: Record<string, string>;
}

/** Typed wrappers over every Rust command (src-tauri/src/commands.rs). Errors arrive as strings. */
export const api = {
  // app
  appInfo: () => invoke<T.AppInfo>('get_app_info'),
  getSettings: () => invoke<T.AppSettings>('get_settings'),
  saveSettings: (settings: T.AppSettings) => invoke<void>('save_settings', { settings }),
  cancelTask: (taskId: string) => invoke<void>('cancel_task', { taskId }),
  testWebhook: (url: string) => invoke<void>('test_webhook', { url }),
  quit: () => invoke<void>('app_quit'),
  hideWindow: () => invoke<void>('window_hide'),
  probeFolder: (path: string) => invoke<T.FolderProbe>('probe_folder', { path }),
  listFolders: () => invoke<T.Folder[]>('list_folders'),
  saveFolders: (folders: T.Folder[]) => invoke<void>('save_folders', { folders }),
  auditLog: (limit = 500) => invoke<T.AuditRow[]>('audit_log', { limit }),
  eventLog: (id: string | null, limit = 200) => invoke<T.EventRow[]>('event_log', { id, limit }),
  metricsHistory: (id: string, since: number) => invoke<T.MetricRow[]>('metrics_history', { id, since }),
  buildHistory: (id: string) => invoke<[string, string, number][]>('build_history', { id }),

  // instances
  listInstances: () => invoke<T.ServerInstance[]>('list_instances'),
  getStatuses: () => invoke<T.ServerStatus[]>('get_statuses'),
  suggestPorts: (excludeId?: string) => invoke<T.PortSet>('suggest_ports', { excludeId: excludeId ?? null }),
  createInstance: (input: CreateInput) => invoke<T.ServerInstance>('create_instance', { input }),
  updateInstance: (instance: T.ServerInstance) => invoke<T.ServerInstance>('update_instance', { instance }),
  reorderInstances: (ids: string[]) => invoke<void>('reorder_instances', { ids }),
  deleteInstance: (id: string, deleteFiles: boolean) => invoke<void>('delete_instance', { id, deleteFiles }),
  cloneInstance: (id: string, name: string, copySaves: boolean) => invoke<T.ServerInstance>('clone_instance', { id, name, copySaves }),
  findExistingInstalls: () => invoke<string[]>('find_existing_installs'),
  importExisting: (path: string, name?: string) => invoke<T.ServerInstance>('import_existing', { path, name: name ?? null }),

  // lifecycle
  installServer: (id: string, validate = false) => invoke<void>('install_server', { id, validate }),
  checkUpdate: (id: string) => invoke<boolean>('check_update', { id }),
  start: (id: string) => invoke<void>('start_server', { id }),
  stop: (id: string, force = false, warn = false) => invoke<void>('stop_server', { id, force, warn }),
  restart: (id: string, update = false, warn = false) => invoke<void>('restart_server', { id, update, warn }),
  cancelCountdown: (id: string) => invoke<void>('cancel_countdown', { id }),
  launchLine: (id: string) => invoke<string>('get_launch_line', { id }),
  previewLaunchLine: (instance: T.ServerInstance) => invoke<string>('preview_launch_line', { instance }),
  setConsoleVisible: (id: string, visible: boolean) => invoke<number>('set_console_visible', { id, visible }),
  clearCrashState: (id: string) => invoke<void>('clear_crash_state', { id }),

  // game rules + config
  readGameRules: (id: string) => invoke<T.GameRules>('read_game_rules', { id }),
  applyGameRules: (id: string, changes: Record<string, string>, liveValues: Record<string, string>, restartKeys: string[]) =>
    invoke<T.ApplyResult>('apply_game_rules', { id, changes, liveValues, restartKeys }),
  resetGameRules: (id: string, keys: string[]) => invoke<void>('reset_game_rules', { id, keys }),
  readConfig: (id: string, file: T.ConfigFileName) => invoke<T.ConfigFile>('read_config', { id, file }),
  /** `base` = the text the editor loaded; the backend merges key-by-key when the file changed since. */
  writeConfig: (id: string, file: T.ConfigFileName, text: string, base?: string) => invoke<{ deferred: boolean }>('write_config', { id, file, text, base }),
  configHistory: (id: string, file: T.ConfigFileName) => invoke<[string, number][]>('list_config_history', { id, file }),
  readTextFile: (id: string, path: string) => invoke<string>('read_text_file', { id, path }),

  // logs + rcon
  logTail: (id: string, maxLines: number, path?: string) => invoke<string[]>('read_log_tail', { id, maxLines, path: path ?? null }),
  logFiles: (id: string) => invoke<T.LogFile[]>('list_log_files', { id }),
  rcon: (id: string, command: string) => invoke<string>('rcon_command', { id, command }),

  // players / world
  playerLists: (id: string) => invoke<T.ListEntry[]>('player_lists', { id }),
  editPlayerList: (id: string, list: 'ban' | 'whitelist', add: boolean, playerId: string, name = '', reason = '') =>
    invoke<string>('edit_player_list', { id, list, add, playerId, name, reason }),
  playerHistory: (id: string) => invoke<T.PlayerSummary[]>('player_history', { id }),
  serverStats: (id: string) => invoke<T.ServerStats | null>('server_stats', { id }),
  listWorlds: (id: string) => invoke<T.WorldSnapshot[]>('list_worlds', { id }),
  restoreWorld: (id: string, name: string) => invoke<void>('restore_world', { id, name }),
  exportWorld: (id: string, name: string, dest: string) => invoke<void>('export_world', { id, name, dest }),
  importWorld: (id: string, path: string) => invoke<void>('import_world', { id, path }),
  deleteWorld: (id: string, name: string) => invoke<void>('delete_world', { id, name }),
  resetWorld: (id: string) => invoke<void>('reset_world', { id }),
  characters: (id: string) => invoke<T.Character[]>('game_characters', { id }),
  clans: (id: string) => invoke<T.Clan[]>('game_clans', { id }),
  gameEvents: (id: string, limit: number, filter: string) => invoke<T.GameEvent[]>('game_events', { id, limit, filter }),
  worldSummary: (id: string) => invoke<T.WorldSummary>('world_summary', { id }),
  listMaps: (id: string) => invoke<T.MapInfo[]>('list_maps', { id }),
  listMapsIn: (installDir: string) => invoke<T.MapInfo[]>('list_maps_in', { installDir }),
  sqlQuery: (id: string, sql: string) => invoke<T.SqlTable>('sql_query', { id, sql }),
  playerAction: (id: string, action: string, userId: string, playerName: string, reason: string) =>
    invoke<string>('player_action', { id, action, userId, playerName, reason }),
  setPlayerNote: (id: string, userId: string, note: string) => invoke<void>('set_player_note', { id, userId, note }),
  moderationLog: (id: string, userId?: string) => invoke<T.ModerationRow[]>('moderation_log', { id, userId: userId ?? null }),

  // mods
  listMods: (id: string, refresh = false) => invoke<T.ModsView>('list_mods', { id, refresh }),
  workshopLookup: (inputs: string[]) => invoke<T.WorkshopItem[]>('workshop_lookup', { inputs }),
  addMods: (id: string, inputs: string[], collection = false) => invoke<T.AddModsResult>('add_mods', { id, inputs, collection }),
  addLocalMod: (id: string, path: string) => invoke<T.ModEntry>('add_local_mod', { id, path }),
  setMods: (id: string, modsList: T.ModEntry[]) => invoke<void>('set_mods', { id, modsList }),
  updateMods: (id: string, restart: boolean) => invoke<void>('update_mods', { id, restart }),
  checkModUpdates: (id: string) => invoke<string[]>('check_mod_updates', { id }),
  modVersions: (workshopId: string) => invoke<T.ArchivedVersion[]>('mod_versions', { workshopId }),
  rollbackMod: (id: string, workshopId: string, stamp: number) => invoke<void>('rollback_mod', { id, workshopId, stamp }),
  setHttpOrigin: (id: string, modFolder: string, origin: string, allow: boolean) => invoke<void>('set_http_origin', { id, modFolder, origin, allow }),
  cleanStrayMods: (id: string) => invoke<string[]>('clean_stray_mods', { id }),
  workshopSearch: (text: string, page: number, enhancedOnly: boolean) => invoke<T.WorkshopItem[]>('workshop_search', { text, page, enhancedOnly }),
  saveModProfile: (id: string, name: string) => invoke<T.ModProfile>('save_mod_profile', { id, name }),
  applyModProfile: (id: string, profileId: string) => invoke<void>('apply_mod_profile', { id, profileId }),
  deleteModProfile: (id: string, profileId: string) => invoke<void>('delete_mod_profile', { id, profileId }),

  // backups + packs
  createBackup: (id: string, label: string, full = false) => invoke<T.BackupInfo>('create_backup', { id, label, full }),
  listBackups: (id: string) => invoke<T.BackupInfo[]>('list_backups', { id }),
  backupDir: (id: string) => invoke<string>('get_backup_dir', { id }),
  verifyBackup: (id: string, path: string) => invoke<T.VerifyReport>('verify_backup', { id, path }),
  restoreBackup: (id: string, path: string, includeConfig: boolean, restoreMods: boolean) =>
    invoke<number>('restore_backup', { id, path, includeConfig, restoreMods }),
  deleteBackup: (id: string, path: string) => invoke<void>('delete_backup', { id, path }),
  exportPack: (id: string, dest: string, options: { includeSaves: boolean; includeSecrets: boolean; includeMods?: boolean }) => invoke<void>('export_pack', { id, dest, options }),
  readPack: (path: string) => invoke<T.PackManifest>('read_pack', { path }),
  importPack: (path: string, name: string, installDir: string | null, installNow: boolean) =>
    invoke<T.ServerInstance>('import_pack', { path, name, installDir, installNow }),

  // network
  firewallState: (id: string) => invoke<T.FirewallState>('firewall_state', { id }),
  addFirewallRules: (id: string) => invoke<void>('add_firewall_rules', { id }),
  removeFirewallRules: (id: string) => invoke<void>('remove_firewall_rules', { id }),
  netInfo: () => invoke<T.NetInfo>('net_info'),
  upnpProbe: () => invoke<T.UpnpInfo>('upnp_probe'),
  upnpMap: (id: string) => invoke<string[]>('upnp_map', { id }),
  upnpUnmap: (id: string) => invoke<void>('upnp_unmap', { id }),

  // diagnostics
  diagnostics: (id: string) => invoke<T.DiagReport>('run_diagnostics', { id }),
  fixBuildOverride: (id: string) => invoke<void>('fix_build_override', { id }),
  installPrereqs: (id: string) => invoke<void>('install_prereqs', { id }),
  diskUsage: (id: string) => invoke<T.DiskUsage>('disk_usage', { id }),
};

export function errMsg(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return JSON.stringify(e);
}
