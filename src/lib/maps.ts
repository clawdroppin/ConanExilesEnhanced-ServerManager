/**
 * Maps a Conan Exiles dedicated server can run.
 *
 * Selection is written to Engine.ini under `[/Script/EngineSettings.GameMapsSettings]` as
 * `ServerDefaultMap=<value>` (see `core/server.rs::sync_ini`). `value` is the game's *internal*
 * map path — not a file path. An empty value means "let the game use its default map" (the
 * manager drops the key so the game falls back).
 *
 * To add a new map (a future official map or a mod map), append an entry here — the create and
 * settings UIs read this list, so it shows up everywhere automatically.
 */
import type { MapInfo } from './types';

export interface GameMap {
  /** Internal map path for `ServerDefaultMap`. Empty = game default. */
  value: string;
  label: string;
  hint: string;
}

export const MAPS: GameMap[] = [
  { value: '', label: 'Game default', hint: 'Let the game choose (currently Exiled Lands)' },
  { value: '/Game/Maps/ConanSandbox/ConanSandbox', label: 'Exiled Lands', hint: 'The original map – desert to frozen north' },
  { value: '/Game/DLC_EXT/DLC_Siptah/Maps/DLC_Isle_of_Siptah', label: 'Isle of Siptah', hint: 'Maelstrom expansion – vaults, surges, elder things' },
];

/** The map a brand-new server should start on (Exiled Lands). */
export const DEFAULT_MAP = '/Game/Maps/ConanSandbox/ConanSandbox';

/** True when `v` is one of the known maps (or empty = game default). */
export function isKnownMap(v: string): boolean {
  return v === '' || MAPS.some((m) => m.value === v);
}

/**
 * Combine the built-in map list with maps discovered in the installed game. Known maps keep their
 * friendly labels; anything new (a future official map or a mod map) is appended automatically, so a
 * newly installed map shows up here with no code change.
 */
export function mergeMaps(fetched: MapInfo[]): GameMap[] {
  const known = new Set(MAPS.map((m) => m.value));
  const extra: GameMap[] = [];
  for (const f of fetched) {
    if (!f.path || known.has(f.path)) continue;
    known.add(f.path);
    extra.push({ value: f.path, label: f.label || f.path, hint: 'Discovered in this install' });
  }
  return [...MAPS, ...extra];
}
