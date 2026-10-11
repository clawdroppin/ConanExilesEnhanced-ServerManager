import { useCallback, useEffect, useState } from 'react';
import { AlertTriangle, CheckCircle2, FileArchive, FolderOpen, FolderPlus, Puzzle, RefreshCw, Trash2, Upload } from 'lucide-react';
import { cn } from '@/lib/cn';
import { api, errMsg } from '@/lib/ipc';
import { bytes, dateTime } from '@/lib/format';
import { pickFiles, pickFolder, reveal } from '@/lib/shell';
import type { ModsState, ModView } from '@/lib/types';
import { useApp, useInstance, useStatus } from '@/store/app';
import { Button, IconButton } from '@/components/ui/Button';
import { confirm } from '@/components/ui/Dialog';
import { Toggle } from '@/components/ui/Field';
import { Badge, Callout, Card, Empty, PageHeader, Section } from '@/components/ui/Surface';

export function Mods({ id }: { id: string }) {
  const inst = useInstance(id)!;
  const st = useStatus(id);
  const toast = useApp((s) => s.toast);
  const [data, setData] = useState<ModsState | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const running = st?.state === 'running' || st?.state === 'starting' || st?.state === 'stopping';

  const load = useCallback(() => {
    api.listMods(id).then(setData).catch((e) => toast('error', errMsg(e)));
  }, [id, toast]);
  useEffect(load, [load]);

  const run = async (key: string, fn: () => Promise<unknown>, ok?: string) => {
    setBusy(key);
    try {
      await fn();
      if (ok) toast('success', ok, id);
    } catch (e) {
      toast('error', errMsg(e), id);
    } finally {
      setBusy(null);
      load();
    }
  };

  const addFiles = async () => {
    const paths = await pickFiles('Choose pak mod files or a .zip', ['pak', 'utoc', 'ucas', 'zip'], 'Pak mods');
    if (paths.length) run('add', async () => { const r = await api.addMods(id, paths); toast('success', `Installed: ${r.added.join(', ')}`, id); });
  };
  const addFolder = async () => {
    const p = await pickFolder('Choose a folder containing pak mods');
    if (p) run('add', async () => { const r = await api.addMods(id, [p]); toast('success', `Installed: ${r.added.join(', ')}`, id); });
  };

  const enabled = data?.mods.filter((m) => m.entry.enabled) ?? [];
  const disabled = data?.mods.filter((m) => !m.entry.enabled) ?? [];
  const broken = data?.mods.filter((m) => !m.complete) ?? [];

  const setEnabled = (mod: ModView, value: boolean) => {
    if (!data) return;
    const updated = data.mods.map((m) => m.key === mod.key ? { ...m, entry: { ...m.entry, enabled: value } } : m);
    run(`t-${mod.entry.name}`, () => api.setMods(id, updated.map((m) => m.entry)), `${mod.entry.name} ${value ? 'enabled' : 'disabled'}`);
  };

  const removeMod = (mod: ModView) => {
    if (!data) return;
    const updated = data.mods.filter((m) => m.key !== mod.key);
    run(`r-${mod.entry.name}`, () => api.setMods(id, updated.map((m) => m.entry)), `${mod.entry.name} removed`);
  };

  return (
    <div className="mx-auto max-w-[1100px] space-y-5 p-6">
      <PageHeader
        title="Mods"
        description="Pak mods (.pak + .utoc + .ucas) load on the server from ConanSandbox\Content\Paks\~mods when it starts. Disabled mods are kept outside the game folder."
        actions={
          <>
            <Button size="sm" icon={<RefreshCw className="size-3.5" />} onClick={load}>
              Refresh
            </Button>
            <Button size="sm" icon={<FolderOpen className="size-3.5" />} disabled={!data} onClick={() => data && reveal(data.folderExists ? data.modsPath : inst.installDir)}>
              Open folder
            </Button>
            <Button size="sm" icon={<FolderPlus className="size-3.5" />} disabled={running} loading={busy === 'add'} onClick={addFolder}>
              Add folder
            </Button>
            <Button size="sm" variant="primary" icon={<Upload className="size-3.5" />} disabled={running} loading={busy === 'add'} onClick={addFiles}>
              Add mods
            </Button>
          </>
        }
      />

      {running && <Callout tone="warn">The server reads mods only when it starts. Stop it to add, enable, disable or remove mods, then start it again.</Callout>}

      <div className="grid grid-cols-3 gap-4">
        <Card className="p-4">
          <div className="text-[11px] font-medium tracking-wide text-fg-3 uppercase">Server state</div>
          <div className="mt-2 flex items-center gap-2">
            {data?.folderExists ? <Badge tone="violet">Modded</Badge> : <Badge tone="ok">Vanilla</Badge>}
            {st?.modded != null && (st.state === 'running' || st.state === 'starting') && (
              <span className="text-[11.5px] text-fg-4">server reports: {st.modded ? 'mods detected' : 'no mods'}</span>
            )}
          </div>
          <div className="mt-1.5 text-[11.5px] text-fg-4">A modded server is flagged as modded in the in-game browser.</div>
        </Card>
        <Card className="p-4">
          <div className="text-[11px] font-medium tracking-wide text-fg-3 uppercase">Enabled</div>
          <div className="mt-1.5 font-mono text-[22px] font-semibold text-fg">{enabled.length}</div>
          <div className="text-[11.5px] text-fg-4">{bytes(enabled.reduce((s, m) => s + m.size, 0))}</div>
        </Card>
        <Card className="p-4">
          <div className="text-[11px] font-medium tracking-wide text-fg-3 uppercase">Problems</div>
          <div className={cn('mt-1.5 font-mono text-[22px] font-semibold', broken.length ? 'text-warn' : 'text-fg')}>{broken.length}</div>
          <div className="text-[11.5px] text-fg-4">{broken.length ? 'incomplete file sets' : 'all sets complete'}</div>
        </Card>
      </div>

      {data && data.folderExists && enabled.length === 0 && (
        <Callout tone="warn" title="Empty ~mods folder">
          The server flags itself as modded whenever the ~mods folder exists – even when it's empty.{' '}
          <button className="text-accent hover:underline" disabled={running} onClick={() => run('clean', () => api.cleanStrayMods(id), 'Removed the empty mods folder')}>
            Remove it
          </button>
        </Callout>
      )}

      <Section title={<span className="flex items-center gap-2">Installed mods <Badge>{data?.mods.length ?? 0}</Badge></span>} icon={<Puzzle />} bodyClassName="py-1">
        {!data ? (
          <div className="py-8 text-center text-[12px] text-fg-4">Loading…</div>
        ) : data.mods.length === 0 ? (
          <Empty
            icon={<Puzzle />}
            title="No mods installed"
            description="Add the .pak/.utoc/.ucas files (or the .zip) a mod ships with. Mods that mention UE4SS, Lua or dwmapi.dll run on players' PCs only."
            action={
              <Button variant="primary" icon={<Upload className="size-3.5" />} disabled={running} onClick={addFiles}>
                Add mods
              </Button>
            }
          />
        ) : (
          <div className="divide-y divide-line">
            {[...enabled, ...disabled].map((m) => (
              <div key={m.key} className={cn('flex items-center gap-3 py-3', !m.entry.enabled && 'opacity-60')}>
                <div className={cn('flex size-9 items-center justify-center rounded-lg', m.entry.enabled ? 'bg-violet/15 text-violet' : 'bg-panel-2 text-fg-4')}>
                  <FileArchive className="size-4" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-[13px] font-medium text-fg">{m.entry.name}</span>
                    {m.complete ? <CheckCircle2 className="size-3.5 text-ok" /> : <AlertTriangle className="size-3.5 text-warn" />}
                    {m.updateAvailable && <Badge tone="info">Update</Badge>}
                  </div>
                  <div className="mt-0.5 font-mono text-[11px] text-fg-4">
                    {bytes(m.size)}{m.localUpdated ? ` · ${dateTime(m.localUpdated * 1000)}` : ''}
                  </div>
                  {m.issues.length > 0 && <div className="mt-1 text-[11.5px] text-warn">{m.issues[0].message}</div>}
                </div>
                <label className="flex items-center gap-2 text-[12px] text-fg-3">
                  {m.entry.enabled ? 'Enabled' : 'Disabled'}
                  <Toggle checked={m.entry.enabled} disabled={running || busy != null} onChange={(v) => setEnabled(m, v)} />
                </label>
                <IconButton
                  label="Remove mod"
                  disabled={running}
                  className="hover:text-err"
                  onClick={async () => {
                    const r = await confirm({ title: `Remove ${m.entry.name}?`, message: 'Its files are moved to the _quarantine folder next to SaveGames, so it can be recovered.', confirm: 'Remove', danger: true });
                    if (r.ok) removeMod(m);
                  }}
                >
                  <Trash2 className="size-3.5" />
                </IconButton>
              </div>
            ))}
          </div>
        )}
      </Section>
    </div>
  );
}
