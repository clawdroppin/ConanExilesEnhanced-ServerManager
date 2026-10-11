import { create } from 'zustand';
import { api, errMsg } from '@/lib/ipc';
import type { ServerInstance } from '@/lib/types';
import { useApp } from '@/store/app';

/** Unsaved edits per server – survive switching tabs/views until saved or discarded. */
const useDrafts = create<{ drafts: Record<string, ServerInstance>; saving: Record<string, boolean> }>(() => ({ drafts: {}, saving: {} }));

function same(a: unknown, b: unknown) {
  return JSON.stringify(a) === JSON.stringify(b);
}

export function useDraft(id: string) {
  const saved = useApp((s) => s.instances.find((i) => i.id === id));
  const draft = useDrafts((s) => s.drafts[id]) ?? saved;
  const saving = useDrafts((s) => !!s.saving[id]);
  const dirty = !!saved && !!draft && !same(saved, draft);

  const patch = (fn: (d: ServerInstance) => void) => {
    const base = structuredClone(useDrafts.getState().drafts[id] ?? useApp.getState().instances.find((i) => i.id === id)!);
    fn(base);
    useDrafts.setState((s) => ({ drafts: { ...s.drafts, [id]: base } }));
  };
  const reset = () =>
    useDrafts.setState((s) => {
      const d = { ...s.drafts };
      delete d[id];
      return { drafts: d };
    });
  const save = async () => {
    const d = useDrafts.getState().drafts[id];
    if (!d) return true;
    useDrafts.setState((s) => ({ saving: { ...s.saving, [id]: true } }));
    try {
      const out = await api.updateInstance(d);
      useApp.getState().upsertInstance(out);
      reset();
      const st = useApp.getState().statuses[id];
      useApp.getState().toast('success', st?.state === 'running' && st.pendingRestart ? 'Saved – restart the server to apply' : 'Settings saved', id);
      return true;
    } catch (e) {
      useApp.getState().toast('error', errMsg(e), id);
      return false;
    } finally {
      useDrafts.setState((s) => ({ saving: { ...s.saving, [id]: false } }));
    }
  };
  return { draft: draft!, saved: saved!, dirty, saving, patch, reset, save };
}

/** Which draft-backed views hold unsaved edits. */
export function dirtyViews(saved: ServerInstance | undefined, draft: ServerInstance | undefined): Set<string> {
  const out = new Set<string>();
  if (!saved || !draft || same(saved, draft)) return out;
  const a = saved.launch;
  const b = draft.launch;
  if (!same(saved.ini, draft.ini) || saved.name !== draft.name || saved.color !== draft.color || saved.notes !== draft.notes || a.port !== b.port || a.queryPort !== b.queryPort || a.maxPlayers !== b.maxPlayers || a.multihome !== b.multihome || a.beaconPort !== b.beaconPort || a.queryPort !== b.queryPort)
    out.add('settings');
  if (!same(a.options, b.options) || a.extraArgs !== b.extraArgs || a.priority !== b.priority || !same(a.affinity, b.affinity) || a.showConsole !== b.showConsole) out.add('launch');
  if (!same(saved.automation, draft.automation)) out.add('automation');
  if (a.upnp !== b.upnp) out.add('ports');
  if (saved.installDir !== draft.installDir || saved.branch !== draft.branch || saved.branchPassword !== draft.branchPassword) out.add('files');
  return out;
}

export function discardDraft(id: string) {
  useDrafts.setState((s) => {
    const d = { ...s.drafts };
    delete d[id];
    return { drafts: d };
  });
}
