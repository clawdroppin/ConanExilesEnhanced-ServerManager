// Command-line switches and engine tunables exposed in Launch & Engine.
//
// The Conan Exiles-specific switches (-Port, -QueryPort, -log, -NewConsole) are documented by Funcom
// and hosting partners; -SteamServerName is read by the game binary. The rest are standard Unreal
// Engine switches/settings – Funcom doesn't document them for Conan Exiles, so the UI labels them as
// engine-level and every one defaults to "not set" (the engine's own default).

import type { ConfigFileName } from '@/lib/types';

export interface LaunchOption {
  key: string;
  kind: 'flag' | 'value';
  label: string;
  description: string;
  default?: boolean;
  placeholder?: string;
  tag?: 'Funcom' | 'Recommended' | 'Engine' | 'Debug';
  warnOff?: string;
}

export const LAUNCH_OPTIONS: LaunchOption[] = [
  {
    key: '-log',
    kind: 'flag',
    label: 'Engine log console',
    description: 'Opens the engine’s log console (hidden by this manager unless you choose to show it). Graceful shutdown works by sending a console break to it.',
    default: true,
    tag: 'Funcom',
    warnOff: 'Without it the manager can’t ask the server to exit cleanly – stops become force-kills.',
  },
  { key: '-NewConsole', kind: 'flag', label: 'Separate console', description: 'Funcom-recommended switch that gives the log its own console window.', tag: 'Funcom' },
  { key: '-unattended', kind: 'flag', label: 'Unattended mode', description: 'Never wait for someone to click a dialog – essential for a headless server.', default: true, tag: 'Recommended' },
  { key: '-NoCrashDialog', kind: 'flag', label: 'No crash dialog (legacy)', description: 'Passed by most hosting panels. The current shipping build doesn’t read it – “Unattended mode” already stops dialogs from blocking – but it’s harmless.', default: true, tag: 'Engine' },
  { key: '-FORCELOGFLUSH', kind: 'flag', label: 'Flush log on every line', description: 'Writes each log line to disk immediately so the live console and player detection update instantly. Tiny disk overhead.', tag: 'Engine' },
  { key: '-LOCALLOGTIMES', kind: 'flag', label: 'Local log timestamps', description: 'Log timestamps in your local time instead of UTC.', tag: 'Engine' },
  { key: '-NOVERIFYGC', kind: 'flag', label: 'Skip GC verification', description: 'Skips garbage-collector reference verification – slightly smoother frame times.', tag: 'Engine' },
  { key: '-SteamServerName', kind: 'value', label: 'Steam server name', description: 'Name advertised to Steam’s server list (the in-game browser uses Server Name).', placeholder: 'My Conan Exiles Server', tag: 'Engine' },
];

export interface EngineSetting {
  file: ConfigFileName;
  section: string;
  key: string;
  label: string;
  description: string;
  type: 'int' | 'float' | 'bool' | 'enum';
  engineDefault: string;
  min?: number;
  max?: number;
  step?: number;
  options?: string[];
  suffix?: string;
  group: 'Networking' | 'Tick & frame rate' | 'Logging';
}

const NET = '/Script/OnlineSubsystemUtils.IpNetDriver';

export const ENGINE_SETTINGS: EngineSetting[] = [
  { file: 'Engine.ini', section: NET, key: 'NetServerMaxTickRate', label: 'Network tick rate', description: 'How many times per second the server sends updates to players. Higher is smoother but uses more CPU and bandwidth.', type: 'int', engineDefault: '30', min: 10, max: 120, suffix: 'Hz', group: 'Networking' },
  { file: 'Engine.ini', section: NET, key: 'MaxClientRate', label: 'Max client rate (LAN)', description: 'Bandwidth cap per player connection on a LAN, in bytes per second.', type: 'int', engineDefault: '15000', min: 2500, max: 1000000, step: 2500, suffix: 'B/s', group: 'Networking' },
  { file: 'Engine.ini', section: NET, key: 'MaxInternetClientRate', label: 'Max client rate (internet)', description: 'Bandwidth cap per player over the internet, in bytes per second. Raise it if large bases replicate slowly.', type: 'int', engineDefault: '10000', min: 2500, max: 1000000, step: 2500, suffix: 'B/s', group: 'Networking' },
  { file: 'Engine.ini', section: NET, key: 'ConnectionTimeout', label: 'Connection timeout', description: 'Seconds without traffic before a connected player is dropped.', type: 'float', engineDefault: '60', min: 10, max: 600, step: 5, suffix: 's', group: 'Networking' },
  { file: 'Engine.ini', section: NET, key: 'InitialConnectTimeout', label: 'Initial connect timeout', description: 'Seconds a joining player has to finish connecting – raise for players on slow connections.', type: 'float', engineDefault: '60', min: 10, max: 600, step: 5, suffix: 's', group: 'Networking' },
  { file: 'Engine.ini', section: '/Script/Engine.Player', key: 'ConfiguredInternetSpeed', label: 'Configured internet speed', description: 'Default bandwidth assumed for internet players (bytes/s).', type: 'int', engineDefault: '10000', min: 2500, max: 1000000, step: 2500, suffix: 'B/s', group: 'Networking' },
  { file: 'Engine.ini', section: '/Script/Engine.Player', key: 'ConfiguredLanSpeed', label: 'Configured LAN speed', description: 'Default bandwidth assumed for LAN players (bytes/s).', type: 'int', engineDefault: '20000', min: 2500, max: 1000000, step: 2500, suffix: 'B/s', group: 'Networking' },
  { file: 'Engine.ini', section: '/Script/Engine.Engine', key: 'bUseFixedFrameRate', label: 'Fixed server frame rate', description: 'Run the simulation at a fixed rate instead of as fast as possible – keeps CPU usage steady.', type: 'bool', engineDefault: 'False', group: 'Tick & frame rate' },
  { file: 'Engine.ini', section: '/Script/Engine.Engine', key: 'FixedFrameRate', label: 'Fixed frame rate', description: 'Target frames per second when the fixed frame rate is on.', type: 'float', engineDefault: '30', min: 10, max: 120, suffix: 'fps', group: 'Tick & frame rate' },
  { file: 'Engine.ini', section: '/Script/Engine.Engine', key: 'bSmoothFrameRate', label: 'Smooth frame rate', description: 'Caps the frame rate to the smoothed range below.', type: 'bool', engineDefault: 'False', group: 'Tick & frame rate' },
  { file: 'Engine.ini', section: 'Core.Log', key: 'LogNet', label: 'Network log detail', description: 'Verbosity of networking messages. The manager detects player joins from “Log” level lines – Warning hides them.', type: 'enum', engineDefault: 'Log', options: ['Verbose', 'Log', 'Display', 'Warning', 'Error'], group: 'Logging' },
  { file: 'Engine.ini', section: 'Core.Log', key: 'LogOnline', label: 'Online services log detail', description: 'Verbosity of EOS/Steam session messages – lower it to quiet a noisy log.', type: 'enum', engineDefault: 'Log', options: ['Verbose', 'Log', 'Display', 'Warning', 'Error'], group: 'Logging' },
  { file: 'Engine.ini', section: 'Core.Log', key: 'Global', label: 'Global log detail', description: 'Default verbosity for every category.', type: 'enum', engineDefault: 'Log', options: ['Verbose', 'Log', 'Display', 'Warning', 'Error'], group: 'Logging' },
];

export const PRIORITIES = [
  { value: 'idle', label: 'Low' },
  { value: 'belowNormal', label: 'Below normal' },
  { value: 'normal', label: 'Normal' },
  { value: 'aboveNormal', label: 'Above normal' },
  { value: 'high', label: 'High' },
];
