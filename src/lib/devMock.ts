// Fake backend for `npm run dev` in a plain browser. Never loaded inside Tauri (see main.tsx).
import { installMock, mockEmit } from './ipc';
import type { AppSettings, BackupInfo, ServerInstance, ServerStatus } from './types';

const now = Date.now();

function makeInstance(id: string, name: string, port: number, color: string, extra: Partial<ServerInstance> = {}): ServerInstance {
  return {
    id,
    name,
    color,
    icon: 'sword',
    installDir: `D:\\ConanExilesServers\\${name.replace(/\W+/g, '-')}`,
    createdAt: now - 86400000 * 3,
    notes: '',
    tags: [],
    folderId: null,
    favorite: false,
    archived: false,
    branch: 'public',
    branchPassword: '',
    identity: { serverName: name.slice(0, 16), serverPassword: '', adminPassword: 'Ember1234', motd: '' },
    ini: { ownerId: '9f2c4e1ab37d4c0e8a1f5b6c7d8e9f01', serverName: name.slice(0, 16), defaultWorldName: name.replace(/\W/g, '').slice(0, 16), adminPassword: 'Ember1234', worldPassword: '', platformPolicy: 'Crossplay', allowCrashDumps: true },
    launch: { port, beaconPort: 0, upnp: false, queryPort: 0, maxPlayers: 16, multihome: '', multihomeHttp: '', showConsole: false, priority: 'normal', affinity: [], options: { '-log': true, '-unattended': true, '-NoCrashDialog': true }, extraArgs: '' },
    rcon: { enabled: false, password: '', port: 0, maxKarma: 0, pollSecs: 5 },
    regions: { excluded: [] },
    mods: [],
    modProfiles: [],
    automation: {
      autoStart: false,
      autoRestart: true,
      restartDelaySecs: 10,
      crashLimit: 3,
      stopTimeoutSecs: 90,
      restartWarnings: [],
      restartMessage: '',
      schedules: [{ enabled: true, time: '05:00', days: [], action: 'restart', payload: '' }],
      rules: [],
      backupIntervalMins: 60,
      backupKeep: 24,
      backupOnStop: true,
      backupBeforeUpdate: true,
      backupIncludeMods: false,
      updateOnStart: false,
      updateCheckMins: 60,
      autoUpdate: false,
      modUpdateCheckMins: 0,
      autoUpdateMods: false,
      updateModsOnStart: false,
      memoryLimitMb: 0,
      maxUptimeHours: 0,
      hangMinutes: 0,
    },
    ...extra,
  };
}

let instances: ServerInstance[] = [makeInstance('a1b2c3d4-0001', 'Ashenfall Friends', 7777, 'ember'), makeInstance('a1b2c3d4-0002', 'Hardcore Test', 7778, 'violet')];

const statuses: Record<string, ServerStatus> = {};
function status(id: string, patch: Partial<ServerStatus> = {}): ServerStatus {
  const s: ServerStatus = statuses[id] ?? {
    id,
    state: 'stopped',
    pid: null,
    startedAt: null,
    readyAt: null,
    installedBuild: '19876543',
    latestBuild: '19876543',
    engineBuild: null,
    engineVersion: '5.4.4-0+++dominion+release',
    gameVersion: null,
    crashCount: 0,
    crashLoop: false,
    pendingRestart: false,
    players: [],
    rconOk: false,
    session: null,
    nextSchedule: { at: now + 3600_000 * 5, action: 'restart' },
    lastBackup: now - 1800_000,
    lastExitCode: null,
    busy: null,
    modsLoaded: [],
    modded: null,
    devkitRevision: null,
    restartPendingAt: null,
  };
  statuses[id] = { ...s, ...patch };
  return statuses[id];
}
status('a1b2c3d4-0001', {
  session: { SlotName: 'AshenfallFriends', JoinCode: 'AGLZ-QCMS', BuildId: '247725' },
  state: 'running',
  pid: 18244,
  startedAt: now - 3600_000 * 2.3,
  readyAt: now - 3600_000 * 2.2,
  players: [
    { idx: 0, charName: 'Gielinor_Gal', playerName: 'Gielinor_Gal', userId: '0a1b2c3d4e5f60718293a4b5c6d7e8f9', platformId: 'EOS:0002a1b2c3', platform: 'EOS' },
    { idx: 1, charName: 'Zezima', playerName: 'Zezima', userId: '1a2b3c4d5e6f70818293a4b5c6d7e8f0', platformId: 'EOS:0003b2c3d4', platform: 'EOS' },
  ],
});
status('a1b2c3d4-0002');

let settings: AppSettings = {
  steamcmdDir: null,
  defaultInstallRoot: null,
  backupDir: null,
  workshopDir: null,
  telemetryIntervalMs: 1000,
  confirmDestructive: true,
  reduceMotion: false,
  compact: false,
  streamerMode: false,
  checkUpdatesOnLaunch: true,
  accent: 'ember',
  closeToTray: true,
  desktopNotifications: true,
  webhookUrl: '',
  webhookLevels: ['success', 'error'],
  steamApiKey: '',
  metricsRetentionDays: 30,
};

const ini = `[/Script/Dominion.DedicatedServerSettings]\r\nOwnerId=9f2c4e1ab37d4c0e8a1f5b6c7d8e9f01\r\nServerName=Ashenfall Friends\r\nDefaultWorldName=AshenfallFriends\r\nAdminPassword=Ember1234\r\nWorldPassword=\r\nPlatformPolicy=Crossplay\r\nbAllowSendingCrashDumps=True\r\nServerGuid=8C1F0E2A4B3D4E5F9A0B1C2D3E4F5061\r\n+KnownPlayerList=(PlayerId="0a1b2c3d4e5f60718293a4b5c6d7e8f9",PlayerName="Zezima")\r\n+KnownPlayerList=(PlayerId="1a2b3c4d5e6f70818293a4b5c6d7e8f0",PlayerName="Gielinor_Gal")\r\n+AdminList=(PlayerId="1a2b3c4d5e6f70818293a4b5c6d7e8f0")\r\n`;

const files: Record<string, string> = { 'DedicatedServer.ini': ini, 'Engine.ini': '[/Script/OnlineSubsystemUtils.IpNetDriver]\r\nNetServerMaxTickRate=30\r\n', 'Game.ini': '[/Script/Engine.GameSession]\r\nMaxPlayers=6\r\n', 'GameUserSettings.ini': '' };

const backups: BackupInfo[] = Array.from({ length: 6 }, (_, i) => ({
  path: `D:\\ConanExilesServers\\Ashenfall-Friends\\Backups\\2026-10-0${9 - i}_auto.zip`,
  name: `2026-10-0${9 - i}_04-00-00_${i === 2 ? 'manual' : 'auto'}.zip`,
  size: 48_000_000 + i * 1_200_000,
  createdAt: now - i * 3600_000 * 7,
  label: i === 2 ? 'Before the dragon fight' : 'Interval',
  kind: i === 2 ? 'manual' : 'auto',
  files: 14,
  verified: true,
  includesMods: false,
  build: '19876543',
}));

const mockMods = [
  { name: 'BiggerBackpacks_P', enabled: true, extensions: ['pak', 'ucas', 'utoc'], size: 3_800_000, modified: now - 86400_000 * 4, complete: true, problem: null as string | null },
  { name: 'FasterSmelting_P', enabled: true, extensions: ['pak', 'ucas', 'utoc'], size: 1_200_000, modified: now - 86400_000 * 2, complete: true, problem: null as string | null },
  { name: 'NoFallDamage', enabled: false, extensions: ['pak', 'utoc'], size: 640_000, modified: now - 86400_000 * 9, complete: false, problem: 'Incomplete set: .ucas is missing. The server logs a missing container and loads nothing from it.' as string | null },
];

const LOG_SAMPLES = [
  'LogNet: Display: SteamNetDriver_0 IpNetDriver listening on port 7777',
  'LogDominion: Autosave complete (world AshenfallFriends) in 412 ms',
  'LogNet: Login request: ?Name=Gielinor_Gal userId: EOS:0002a1b2c3 platform: EOS',
  'LogNet: Join succeeded: Gielinor_Gal',
  'LogWorldPartition: Display: Streaming cell loaded in 23ms',
  'LogEOS: Warning: [LogEOSSessions] UpdateSession: rate limited, retrying',
  'LogSpud: Saving world state… 1932 actors',
  'LogTemp: Display: Weather changed to Storm',
];

function emitTelemetry() {
  const t = Date.now() / 1000;
  const running = instances.filter((i) => statuses[i.id]?.state === 'running' || statuses[i.id]?.state === 'starting');
  mockEmit('telemetry', {
    ts: Date.now(),
    system: {
      cpu: 22 + Math.sin(t / 7) * 8 + Math.random() * 6,
      memUsed: 14.2e9 + Math.sin(t / 30) * 4e8,
      memTotal: 34.2e9,
      rxBps: 180_000 + Math.random() * 90_000,
      txBps: 260_000 + Math.random() * 120_000,
      cores: 16,
      perCore: Array.from({ length: 16 }, (_, i) => Math.max(1, 25 + Math.sin(t / 3 + i) * 20 + Math.random() * 12)),
    },
    servers: running.map((i, k) => ({
      id: i.id,
      cpu: 6 + k * 3 + Math.sin(t / 5) * 2.5 + Math.random() * 2,
      mem: 3.1e9 + k * 6e8 + Math.sin(t / 40) * 1.2e8,
      virt: 5.6e9,
      readBps: Math.random() < 0.2 ? Math.random() * 2e6 : Math.random() * 40_000,
      writeBps: Math.random() < 0.1 ? 4e6 + Math.random() * 3e6 : Math.random() * 120_000,
      uptime: Math.round((Date.now() - (statuses[i.id].startedAt ?? Date.now())) / 1000),
    })),
  });
}

function emitStatus(id: string, patch: Partial<ServerStatus>) {
  mockEmit('server-status', status(id, patch));
}

function log(id: string, source: string, line: string) {
  mockEmit('log-lines', { id, source, lines: [line], ts: Date.now() });
}

export function installDevMock() {
  const find = (id: unknown) => instances.find((i) => i.id === id)!;
  installMock({
    get_app_info: () => ({ version: '1.0.0', dataDir: 'C:\\Users\\me\\AppData\\Roaming\\com.conanexilesservermanager.app', portable: false, appId: 4019830, cores: 16, totalMemory: 34.2e9, os: 'Windows 11 Pro', hostname: 'FORGE-PC' }),
    get_settings: () => settings,
    save_settings: ({ settings: s }) => void (settings = s as AppSettings),
    cancel_task: () => undefined,
    test_webhook: () => undefined,
    app_quit: () => undefined,
    window_hide: () => undefined,
    probe_folder: () => ({ exists: true, writable: true, empty: true, hasServer: false, freeBytes: 812e9 }),
    list_instances: () => instances,
    get_statuses: () => instances.map((i) => status(i.id)),
    suggest_ports: () => ({ game: 7777 + instances.length, query: 7778 + instances.length, rcon: 7779 + instances.length }),
    create_instance: ({ input }) => {
      const inp = input as { name: string; color: string; ini: ServerInstance['ini']; port: number; maxPlayers: number; installNow: boolean };
      const inst = makeInstance(`mock-${Date.now()}`, inp.name, inp.port, inp.color);
      inst.ini = inp.ini;
      inst.launch.maxPlayers = inp.maxPlayers;
      instances = [...instances, inst];
      status(inst.id, { state: inp.installNow ? 'installing' : 'notInstalled', installedBuild: null });
      if (inp.installNow) {
        const task = `t-${Date.now()}`;
        let p = 0;
        const iv = setInterval(() => {
          p += 0.08;
          mockEmit('task', { taskId: task, instanceId: inst.id, kind: 'steamcmd', title: `Installing ${inst.name}`, message: `Downloading – ${(p * 100).toFixed(1)}% (${(p * 4.2).toFixed(2)} / 4.20 GB)`, progress: Math.min(1, p), done: p >= 1, error: null });
          if (p >= 1) {
            clearInterval(iv);
            emitStatus(inst.id, { state: 'stopped', installedBuild: '19876543' });
          }
        }, 400);
      }
      return inst;
    },
    update_instance: ({ instance }) => {
      const i = instance as ServerInstance;
      instances = instances.map((x) => (x.id === i.id ? i : x));
      if (statuses[i.id]?.state === 'running') emitStatus(i.id, { pendingRestart: true });
      return i;
    },
    reorder_instances: () => undefined,
    delete_instance: ({ id }) => void (instances = instances.filter((i) => i.id !== id)),
    clone_instance: ({ id, name }) => {
      const c = { ...structuredClone(find(id)), id: `mock-${Date.now()}`, name: name as string, launch: { ...find(id).launch, port: 7777 + instances.length } };
      instances = [...instances, c];
      status(c.id, { state: 'notInstalled', installedBuild: null });
      return c;
    },
    find_existing_installs: () => ['G:\\SteamLibrary\\steamapps\\common\\ConanExiles Dedicated Server'],
    import_existing: ({ path }) => {
      const c = makeInstance(`mock-${Date.now()}`, 'Imported Server', 7779, 'sky', { installDir: path as string });
      instances = [...instances, c];
      status(c.id);
      return c;
    },
    install_server: ({ id }) => {
      emitStatus(id as string, { state: 'updating' });
      setTimeout(() => emitStatus(id as string, { state: 'stopped' }), 2500);
    },
    check_update: () => false,
    start_server: ({ id }) => {
      const sid = id as string;
      emitStatus(sid, { state: 'starting', pid: 4242, startedAt: Date.now(), players: [] });
      log(sid, 'manager', 'Launching: ConanSandboxServer-Win64-Shipping.exe -Port=7778 -ini:Game:[/Script/Engine.GameSession]:MaxPlayers=6 -log -unattended -NoCrashDialog');
      setTimeout(() => {
        log(sid, 'server', 'LogNet: Display: SteamNetDriver_0 IpNetDriver listening on port 7778');
        emitStatus(sid, { state: 'running', readyAt: Date.now() });
        mockEmit('server-event', { id: sid, level: 'success', message: `${find(sid).name} is online`, ts: Date.now() });
      }, 2500);
    },
    stop_server: ({ id }) => {
      const sid = id as string;
      emitStatus(sid, { state: 'stopping' });
      log(sid, 'manager', 'Asking the server to shut down cleanly…');
      setTimeout(() => {
        log(sid, 'manager', 'Server stopped cleanly.');
        emitStatus(sid, { state: 'stopped', pid: null, startedAt: null, players: [], pendingRestart: false });
      }, 1800);
    },
    restart_server: ({ id }) => {
      const sid = id as string;
      emitStatus(sid, { state: 'restarting' });
      setTimeout(() => emitStatus(sid, { state: 'running', startedAt: Date.now(), pendingRestart: false }), 3000);
    },
    get_launch_line: ({ id }) => `"${find(id).installDir}\\ConanSandbox\\Binaries\\Win64\\ConanSandboxServer-Win64-Shipping.exe" -Port=${find(id).launch.port} -log`,
    preview_launch_line: ({ instance }) => {
      const i = instance as ServerInstance;
      const opts = Object.entries(i.launch.options)
        .filter(([, v]) => v !== false && v !== '')
        .map(([k, v]) => (v === true ? k : `${k}=${v}`));
      return `"${i.installDir}\\ConanSandbox\\Binaries\\Win64\\ConanSandboxServer-Win64-Shipping.exe" -Port=${i.launch.port}${i.launch.queryPort ? ` -QueryPort=${i.launch.queryPort}` : ''}${i.launch.multihome ? ` -MULTIHOME=${i.launch.multihome}` : ''} -ini:Game:[/Script/Engine.GameSession]:MaxPlayers=${i.launch.maxPlayers} ${opts.join(' ')} ${i.launch.extraArgs}`.trim();
    },
    set_console_visible: () => 1,
    clear_crash_state: () => undefined,
    read_config: ({ file }) => ({ path: `D:\\ConanExilesServers\\Ashenfall-Friends\\ConanSandbox\\Saved\\Config\\WindowsServer\\${file}`, exists: true, text: files[file as string] ?? '', modified: now - 60000 }),
    write_config: ({ id, file, text }) => {
      files[file as string] = text as string;
      return { deferred: statuses[id as string]?.state === 'running', instance: find(id) };
    },
    list_config_history: () => [['D:\\x\\_history\\DedicatedServer.ini.20261009-101500', now - 3600_000]],
    read_text_file: () => LOG_SAMPLES.join('\n'),
    read_log_tail: () => ['LogInit: Build: ++dominion+release-CL-19876543', 'LogInit: Engine Version: 5.4.4-0+++dominion+release', ...LOG_SAMPLES],
    list_log_files: () => [{ path: 'D:\\x\\ConanSandbox-backup-2026.10.08.log', name: 'ConanSandbox-backup-2026.10.08-21.04.11.log', size: 2_400_000, modified: now - 86400_000 }],
    list_saves: () => [
      { name: 'AshenfallFriends.sav', path: 'D:\\x\\AshenfallFriends.sav', size: 38_400_000, modified: now - 120_000, active: true },
      { name: 'OldWorld.sav', path: 'D:\\x\\OldWorld.sav', size: 21_900_000, modified: now - 86400_000 * 9, active: false },
      { name: 'Players\\0a1b2c3d.sav', path: 'D:\\x\\Players\\0a1b2c3d.sav', size: 900_000, modified: now - 300_000, active: false },
    ],
    list_local_saves: () => [{ name: 'MySoloWorld.sav', path: 'C:\\Users\\me\\AppData\\Local\\ConanSandbox\\Saved\\SaveGames\\MySoloWorld.sav', size: 12_400_000, modified: now - 86400_000 * 2, active: false }],
    activate_save: () => undefined,
    import_save: () => ({ name: 'MySoloWorld.sav', path: '', size: 1, modified: Date.now(), active: true }),
    export_save: () => undefined,
    delete_save: () => undefined,
    rename_save: () => undefined,
    reset_world: () => null,
    player_lists: () => [
      { list: 'KnownPlayerList', raw: '(PlayerId="0a1b",PlayerName="Zezima")', fields: [['PlayerId', '0a1b2c3d4e5f60718293a4b5c6d7e8f9'], ['PlayerName', 'Zezima']] },
      { list: 'KnownPlayerList', raw: '(PlayerId="1a2b",PlayerName="Gielinor_Gal")', fields: [['PlayerId', '1a2b3c4d5e6f70818293a4b5c6d7e8f0'], ['PlayerName', 'Gielinor_Gal']] },
      { list: 'AdminList', raw: '(PlayerId="1a2b")', fields: [['PlayerId', '1a2b3c4d5e6f70818293a4b5c6d7e8f0']] },
      { list: 'BannedUserList', raw: '(PlayerId="ffee",PlayerName="GoldSpammer")', fields: [['PlayerId', 'ffeeddccbbaa99887766554433221100'], ['PlayerName', 'GoldSpammer']] },
    ],
    remove_list_entry: () => true,
    list_mods: () => ({
      mods: mockMods,
      folderExists: mockMods.some((m) => m.enabled),
      strayFiles: [],
      modsPath: 'D:\\ConanExilesServers\\Ashenfall-Friends\\ConanSandbox\\Content\\Paks\\~mods',
    }),
    add_mods: ({ paths }) => {
      const names = (paths as string[]).map((p) => p.split(/[\\/]/).pop()!.replace(/\.[^.]+$/, ''));
      names.forEach((n) => mockMods.push({ name: n, enabled: true, extensions: ['pak', 'ucas', 'utoc'], size: 4_200_000, modified: Date.now(), complete: true, problem: null }));
      return names;
    },
    set_mod_enabled: ({ name, enabled }) => {
      const m = mockMods.find((x) => x.name === name);
      if (m) m.enabled = enabled as boolean;
    },
    remove_mod: ({ name }) => {
      const i = mockMods.findIndex((x) => x.name === name);
      if (i >= 0) mockMods.splice(i, 1);
    },
    clean_stray_mods: () => 0,
    create_backup: ({ label }) => {
      const b = { ...backups[0], name: 'new.zip', path: `D:\\x\\${Date.now()}.zip`, createdAt: Date.now(), label: (label as string) || 'Manual backup', kind: 'manual' };
      backups.unshift(b);
      return b;
    },
    list_backups: () => backups,
    get_backup_dir: () => 'D:\\ConanExilesServers\\Ashenfall-Friends\\Backups',
    restore_backup: () => 14,
    delete_backup: ({ path }) => {
      const i = backups.findIndex((b) => b.path === path);
      if (i >= 0) backups.splice(i, 1);
    },
    export_pack: () => undefined,
    read_pack: () => ({ format: 1, appVersion: '1.0.0', exportedAt: now - 86400_000, instance: makeInstance('x', 'Friends Server', 7777, 'gold'), hasSaves: true, hasConfigs: true, includesSecrets: false }),
    import_pack: ({ name }) => {
      const c = makeInstance(`mock-${Date.now()}`, (name as string) || 'Imported', 7780, 'gold');
      instances = [...instances, c];
      status(c.id, { state: 'notInstalled', installedBuild: null });
      return c;
    },
    firewall_state: () => ({ ruleExists: true, portsOk: true, rulePorts: '7777', programRule: true, blockedBy: [] }),
    add_firewall_rules: () => undefined,
    remove_firewall_rules: () => undefined,
    net_info: () => ({ publicIp: '203.0.113.42', local: [{ interface: 'Ethernet', ip: '192.168.1.20' }], gateway: '192.168.1.1' }),
    upnp_probe: () => ({
      found: true,
      gateway: '192.168.1.1',
      routerName: 'Archer AX55',
      manufacturer: 'TP-Link',
      model: 'AX3000',
      externalIp: '203.0.113.42',
      wanKind: 'public',
      mappings: [{ externalPort: 7777, internalPort: 7777, internalClient: '192.168.1.20', description: 'Conan Exiles Ashenfall Friends (7777)', enabled: true }],
      error: null,
    }),
    upnp_map: ({ id }) => {
      const i = find(id);
      const b = i.launch.beaconPort || i.launch.port + 1111;
      return [`UDP ${i.launch.port} → 192.168.1.20:${i.launch.port} forwarded`, `UDP ${b} → 192.168.1.20:${b} forwarded`];
    },
    upnp_unmap: () => undefined,
    run_diagnostics: () => [
      { id: 'install', title: 'Server files', level: 'ok', detail: 'D:\\ConanExilesServers\\Ashenfall-Friends\\ConanSandbox\\Binaries\\Win64\\ConanSandboxServer-Win64-Shipping.exe', fix: '' },
      { id: 'required', title: 'Required settings', level: 'ok', detail: 'Server name, world name and admin password are set.', fix: '' },
      { id: 'owner', title: 'Owner ID', level: 'ok', detail: 'Looks like a valid Player ID.', fix: '' },
      { id: 'ports', title: 'Ports', level: 'ok', detail: 'UDP 7777 is available (used by this server).', fix: '' },
      { id: 'firewall', title: 'Windows Firewall', level: 'warn', detail: 'No inbound rule for this server – players outside your PC may not reach it.', fix: 'firewall' },
      { id: 'ram', title: 'Memory', level: 'ok', detail: '31.9 GB installed – 8 GB recommended for 6 players.', fix: '' },
      { id: 'disk', title: 'Disk space', level: 'ok', detail: '812.0 GB free on D:\\.', fix: '' },
      { id: 'vcredist', title: 'Visual C++ runtime', level: 'ok', detail: 'Installed.', fix: '' },
    ],
    install_prereqs: () => undefined,
    disk_usage: () => ({ install: 5.2e9, saves: 62e6, backups: 310e6, logs: 18e6, mods: 140e6 }),

    // missing mocks
    audit_log: () => [],
    event_log: () => [],
    metrics_history: () => [],
    build_history: () => [],
    list_folders: () => [],
    save_folders: () => undefined,
    cancel_countdown: () => undefined,
    read_game_rules: () => ({}),
    apply_game_rules: () => ({ deferred: false, instance: find('x') }),
    reset_game_rules: () => undefined,
    rcon_command: () => 'ok',
    player_history: () => [],
    game_characters: () => [],
    game_clans: () => [],
    game_events: () => [],
    world_summary: () => ({ players: 0, characters: 0, clans: 0, events: 0, dbVersion: '1.0', serverRuntimeSecs: 0 }),
    sql_query: () => ({ columns: [], rows: [] }),
    player_action: () => 'ok',
    set_player_note: () => undefined,
    moderation_log: () => [],
    add_local_mod: () => ({ name: 'test', enabled: true, extensions: ['pak'], size: 1000, modified: Date.now(), complete: true, problem: null }),
    set_mods: () => undefined,
    update_mods: () => undefined,
    check_mod_updates: () => [],
    mod_versions: () => [],
    rollback_mod: () => undefined,
    set_http_origin: () => undefined,
    workshop_lookup: () => [],
    workshop_search: () => [],
    save_mod_profile: () => ({ id: 'p1', name: 'Test', createdAt: Date.now() }),
    apply_mod_profile: () => undefined,
    delete_mod_profile: () => undefined,
    verify_backup: () => ({ ok: true, files: 10, missing: [] }),
    fix_build_override: () => undefined,
  });

  setInterval(emitTelemetry, 1000);
  let k = 0;
  setInterval(() => {
    for (const i of instances) if (statuses[i.id]?.state === 'running') log(i.id, 'server', `[${new Date().toISOString().replace('T', '-').slice(0, 19)}] ${LOG_SAMPLES[k++ % LOG_SAMPLES.length]}`);
  }, 1700);
}
