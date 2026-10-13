import { create } from 'zustand';
import { api } from '@/lib/ipc';
import type { AppInfo, AppSettings, Folder, PlayerEvent, ServerInstance, ServerStatus, TaskEvent } from '@/lib/types';

export type ViewId =
  | 'overview'
  | 'performance'
  | 'players'
  | 'console'
  | 'mods'
  | 'backups'
  | 'automation'
  | 'worlds'
  | 'config'
  | 'health'
  | 'ports'
  | 'files'
  | 'settings'
  | 'rules'
  | 'launch';

export interface Toast {
  id: number;
  level: 'info' | 'success' | 'warn' | 'error';
  message: string;
  serverId?: string;
}

export interface Task extends TaskEvent {
  startedAt: number;
  updatedAt: number;
}

type Dialog =
  | { kind: 'new'; templateId?: string }
  | { kind: 'import'; tab?: 'pack' | 'folder' }
  | { kind: 'export'; id: string }
  | { kind: 'clone'; id: string }
  | { kind: 'delete'; id: string }
  | { kind: 'settings'; tab?: string }
  | null;

interface AppStore {
  ready: boolean;
  info: AppInfo | null;
  settings: AppSettings | null;
  instances: ServerInstance[];
  statuses: Record<string, ServerStatus>;
  tabs: string[];
  active: string; // 'home' or a server id
  views: Record<string, ViewId>;
  tasks: Record<string, Task>;
  toasts: Toast[];
  players: Record<string, PlayerEvent[]>;
  dialog: Dialog;
  palette: boolean;
  folders: Folder[];
  /** Bumped when mods change in the backend (downloads, updates) so views refetch. */
  modsVersion: number;

  init(): Promise<void>;
  refresh(): Promise<void>;
  upsertInstance(i: ServerInstance): void;
  removeInstance(id: string): void;
  setStatus(s: ServerStatus): void;
  openServer(id: string, view?: ViewId): void;
  closeTab(id: string): void;
  setActive(id: string): void;
  moveTab(from: number, to: number): void;
  setView(id: string, v: ViewId): void;
  applyTask(t: TaskEvent): void;
  dismissTask(id: string): void;
  toast(level: Toast['level'], message: string, serverId?: string): void;
  dismissToast(id: number): void;
  addPlayerEvent(e: PlayerEvent): void;
  setDialog(d: Dialog): void;
  setPalette(open: boolean): void;
  saveSettings(s: AppSettings): Promise<void>;
  saveFolders(f: Folder[]): Promise<void>;
  bumpMods(): void;
}

const LS_KEY = 'cesm.ui';

function loadUi(): { tabs: string[]; active: string; views: Record<string, ViewId> } {
  try {
    const v = JSON.parse(localStorage.getItem(LS_KEY) ?? '{}');
    return { tabs: Array.isArray(v.tabs) ? v.tabs : [], active: typeof v.active === 'string' ? v.active : 'home', views: v.views ?? {} };
  } catch {
    return { tabs: [], active: 'home', views: {} };
  }
}

function saveUi(s: Pick<AppStore, 'tabs' | 'active' | 'views'>) {
  try {
    localStorage.setItem(LS_KEY, JSON.stringify({ tabs: s.tabs, active: s.active, views: s.views }));
  } catch {
    /* storage unavailable */
  }
}

let toastSeq = 0;

function applyUiPrefs(s: AppSettings) {
  const root = document.documentElement;
  root.classList.toggle('reduce-motion', s.reduceMotion);
  root.classList.toggle('compact', s.compact);
  root.classList.toggle('streamer', s.streamerMode);
  root.dataset.accent = s.accent || 'bronze';
}

export const useApp = create<AppStore>((set, get) => ({
  ready: false,
  info: null,
  settings: null,
  instances: [],
  statuses: {},
  ...loadUi(),
  tasks: {},
  toasts: [],
  players: {},
  dialog: null,
  palette: false,
  folders: [],
  modsVersion: 0,

  async init() {
    const [info, settings, instances, statuses, folders] = await Promise.all([
      api.appInfo(),
      api.getSettings(),
      api.listInstances(),
      api.getStatuses(),
      api.listFolders(),
    ]);
    const ids = new Set(instances.map((i) => i.id));
    const tabs = get().tabs.filter((t) => ids.has(t));
    const active = get().active === 'home' || ids.has(get().active) ? get().active : 'home';
    set({
      info,
      settings,
      instances,
      statuses: Object.fromEntries(statuses.map((s) => [s.id, s])),
      tabs,
      active,
      folders,
      ready: true,
    });
    applyUiPrefs(settings);
  },

  async refresh() {
    try {
      const [instances, statuses] = await Promise.all([api.listInstances(), api.getStatuses()]);
      set({ instances, statuses: { ...get().statuses, ...Object.fromEntries(statuses.map((s) => [s.id, s])) } });
    } catch (e) {
      console.error('[refresh] failed:', e);
    }
  },

  upsertInstance(i) {
    const list = get().instances;
    const idx = list.findIndex((x) => x.id === i.id);
    set({ instances: idx >= 0 ? list.map((x) => (x.id === i.id ? i : x)) : [...list, i] });
  },

  removeInstance(id) {
    const s = get();
    const tabs = s.tabs.filter((t) => t !== id);
    const statuses = { ...s.statuses };
    delete statuses[id];
    const next = { instances: s.instances.filter((i) => i.id !== id), tabs, statuses, active: s.active === id ? 'home' : s.active };
    set(next);
    saveUi({ ...get() });
  },

  setStatus(st) {
    set({ statuses: { ...get().statuses, [st.id]: st } });
  },

  openServer(id, view) {
    const s = get();
    const tabs = s.tabs.includes(id) ? s.tabs : [...s.tabs, id];
    const views = view ? { ...s.views, [id]: view } : s.views;
    set({ tabs, active: id, views });
    saveUi(get());
  },

  closeTab(id) {
    const s = get();
    const idx = s.tabs.indexOf(id);
    const tabs = s.tabs.filter((t) => t !== id);
    let active = s.active;
    if (active === id) active = tabs[Math.min(idx, tabs.length - 1)] ?? 'home';
    set({ tabs, active });
    saveUi(get());
  },

  setActive(id) {
    set({ active: id });
    saveUi(get());
  },

  moveTab(from, to) {
    const tabs = [...get().tabs];
    const [t] = tabs.splice(from, 1);
    tabs.splice(to, 0, t);
    set({ tabs });
    saveUi(get());
  },

  setView(id, v) {
    set({ views: { ...get().views, [id]: v } });
    saveUi(get());
  },

  applyTask(t) {
    const prev = get().tasks[t.taskId];
    const now = Date.now();
    set({ tasks: { ...get().tasks, [t.taskId]: { ...t, startedAt: prev?.startedAt ?? now, updatedAt: now } } });
    if (t.done) {
      get().toast(t.error ? 'error' : 'success', t.error ? `${t.title}: ${t.error}` : t.message || t.title, t.instanceId ?? undefined);
      setTimeout(() => get().dismissTask(t.taskId), t.error ? 12000 : 4000);
    }
  },

  dismissTask(id) {
    const tasks = { ...get().tasks };
    delete tasks[id];
    set({ tasks });
  },

  toast(level, message, serverId) {
    const id = ++toastSeq;
    set({ toasts: [...get().toasts.slice(-4), { id, level, message, serverId }] });
    setTimeout(() => get().dismissToast(id), level === 'error' ? 9000 : 4500);
  },

  dismissToast(id) {
    set({ toasts: get().toasts.filter((t) => t.id !== id) });
  },

  addPlayerEvent(e) {
    const prev = get().players[e.id] ?? [];
    set({ players: { ...get().players, [e.id]: [...prev.slice(-199), e] } });
  },

  setDialog(dialog) {
    set({ dialog });
  },

  setPalette(palette) {
    set({ palette });
  },

  async saveSettings(s) {
    await api.saveSettings(s);
    set({ settings: s });
    applyUiPrefs(s);
  },

  async saveFolders(f) {
    set({ folders: f });
    await api.saveFolders(f);
  },

  bumpMods() {
    set({ modsVersion: get().modsVersion + 1 });
  },
}));

export const useInstance = (id: string) => useApp((s) => s.instances.find((i) => i.id === id));
export const useStatus = (id: string) => useApp((s) => s.statuses[id]);

export function isLive(state: ServerStatus['state'] | undefined) {
  return state === 'starting' || state === 'running' || state === 'stopping' || state === 'restarting';
}

export function isBusy(state: ServerStatus['state'] | undefined) {
  return state === 'starting' || state === 'stopping' || state === 'restarting' || state === 'installing' || state === 'updating';
}
