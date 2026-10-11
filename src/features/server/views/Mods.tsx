import { useCallback, useEffect, useState } from 'react';
import { AlertTriangle, CheckCircle2, Download, FileArchive, FolderOpen, Puzzle, RefreshCw, Search, Trash2, Upload, XCircle } from 'lucide-react';
import { cn } from '@/lib/cn';
import { api, errMsg } from '@/lib/ipc';
import { bytes, dateTime } from '@/lib/format';
import { pickFiles, reveal } from '@/lib/shell';
import type { AddModsResult, ModsState, ModView, WorkshopItem } from '@/lib/types';
import { useApp, useInstance, useStatus } from '@/store/app';
import { Button, IconButton } from '@/components/ui/Button';
import { Checkbox, Input, TextArea, Toggle } from '@/components/ui/Field';
import { confirm } from '@/components/ui/Dialog';
import { Badge, Callout, Card, Empty, PageHeader, Section } from '@/components/ui/Surface';

/** Split pasted text into individual Workshop ids/links. Tolerates newlines, spaces and commas. */
function parseInputs(text: string): string[] {
  return text.split(/[\s,]+/).map((s) => s.trim()).filter(Boolean);
}

export function Mods({ id }: { id: string }) {
  const inst = useInstance(id)!;
  const st = useStatus(id);
  const toast = useApp((s) => s.toast);
  const settings = useApp((s) => s.settings);
  const [data, setData] = useState<ModsState | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const running = st?.state === 'running' || st?.state === 'starting' || st?.state === 'stopping';

  // Install-from-Workshop state
  const [paste, setPaste] = useState('');
  const [asCollection, setAsCollection] = useState(false);
  const [installResult, setInstallResult] = useState<AddModsResult | null>(null);

  // Browse-the-Workshop state
  const [searchText, setSearchText] = useState('');
  const [searchResults, setSearchResults] = useState<WorkshopItem[] | null>(null);
  const [searchBusy, setSearchBusy] = useState(false);
  const [enhancedOnly, setEnhancedOnly] = useState(true);
  const [addingId, setAddingId] = useState<string | null>(null);
  const [addedIds, setAddedIds] = useState<Set<string>>(new Set());

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

  const addLocal = async () => {
    const paths = await pickFiles('Choose a .pak mod file', ['pak'], 'Pak mod');
    if (!paths.length) return;
    setBusy('add');
    let added = 0;
    try {
      for (const p of paths) {
        await api.addLocalMod(id, p);
        added++;
      }
      toast('success', `Added ${added} local mod${added > 1 ? 's' : ''}`, id);
    } catch (e) {
      toast('error', errMsg(e), id);
    } finally {
      setBusy(null);
      load();
    }
  };

  const installFromWorkshop = async () => {
    const inputs = parseInputs(paste);
    if (inputs.length === 0) { toast('error', 'Paste at least one Workshop id or link.', id); return; }
    setBusy('install');
    setInstallResult(null);
    try {
      const r = await api.addMods(id, inputs, asCollection);
      setInstallResult(r);
      if (r.added.length) { setPaste(''); toast('success', `Installed: ${r.added.join(', ')}`, id); }
      else toast('warn', 'Nothing was added — see the result below.', id);
    } catch (e) {
      toast('error', errMsg(e), id);
    } finally {
      setBusy(null);
      load();
    }
  };

  const doSearch = async () => {
    if (!searchText.trim()) { toast('error', 'Type something to search for.', id); return; }
    setSearchBusy(true);
    setSearchResults(null);
    setAddedIds(new Set());
    try {
      const r = await api.workshopSearch(searchText.trim(), 1, enhancedOnly);
      setSearchResults(r);
      if (r.length === 0) toast('warn', 'No Workshop results found.', id);
    } catch (e) {
      toast('error', errMsg(e), id);
    } finally {
      setSearchBusy(false);
    }
  };

  const addResult = async (item: WorkshopItem) => {
    setAddingId(item.id);
    try {
      const r = await api.addMods(id, [item.id], false);
      if (r.added.length) {
        setAddedIds((prev) => new Set(prev).add(item.id));
        toast('success', `Installed: ${r.added.join(', ')}`, id);
      } else {
        toast('warn', `Skipped: ${r.skipped.join('; ') || 'already installed'}`, id);
      }
    } catch (e) {
      toast('error', errMsg(e), id);
    } finally {
      setAddingId(null);
      load();
    }
  };

  const enabled = data?.mods.filter((m) => m.entry.enabled) ?? [];
  const disabled = data?.mods.filter((m) => !m.entry.enabled) ?? [];
  const broken = data?.mods.filter((m) => !m.complete) ?? [];
  const updates = data?.mods.filter((m) => m.updateAvailable) ?? [];

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
            <Button size="sm" variant="primary" icon={<Upload className="size-3.5" />} disabled={running} loading={busy === 'add'} onClick={addLocal}>
              Add local .pak
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

      <Section
        title={<span className="flex items-center gap-2">Install from Steam Workshop</span>}
        description="Paste one or more Workshop ids or links (one per line). The manager resolves them, adds the valid Conan Exiles mods to the load order, and downloads them in the background."
        icon={<Download />}
        actions={
          <Button
            size="sm"
            variant="primary"
            icon={<Download className="size-3.5" />}
            disabled={running || busy === 'install' || parseInputs(paste).length === 0}
            loading={busy === 'install'}
            onClick={installFromWorkshop}
          >
            Install
          </Button>
        }
        bodyClassName="py-4"
      >
        <div className="space-y-3">
          <TextArea
            value={paste}
            onChange={(e) => { setPaste(e.target.value); setInstallResult(null); }}
            placeholder={'Paste Workshop ids or links, one per line, e.g.\n3054029483\nhttps://steamcommunity.com/sharedfiles/filedetails/?id=3054029483'}
            rows={3}
            spellCheck={false}
            className="font-mono text-[12px]"
          />
          <div className="flex flex-wrap items-center justify-between gap-3">
            <Checkbox checked={asCollection} onChange={setAsCollection} label="Treat each entry as a collection (install every mod inside it)" />
            <span className="text-[11.5px] text-fg-4">No Steam API key needed — uses public Workshop data + anonymous SteamCMD.</span>
          </div>

          {installResult && (
            <div className="space-y-2 rounded-lg border border-line bg-panel-2/50 p-3 text-[12px]">
              {installResult.added.length > 0 && (
                <div className="flex items-start gap-2 text-ok">
                  <CheckCircle2 className="mt-0.5 size-3.5 shrink-0" />
                  <span><span className="font-medium">Added:</span> {installResult.added.join(', ')}</span>
                </div>
              )}
              {installResult.skipped.length > 0 && (
                <div className="flex items-start gap-2 text-fg-3">
                  <XCircle className="mt-0.5 size-3.5 shrink-0" />
                  <span><span className="font-medium">Skipped:</span> {installResult.skipped.join('; ')}</span>
                </div>
              )}
              {installResult.missingDependencies.length > 0 && (
                <div className="flex items-start gap-2 text-warn">
                  <AlertTriangle className="mt-0.5 size-3.5 shrink-0" />
                  <span>
                    <span className="font-medium">May need these dependencies</span> (paste the ids to install):{' '}
                    <span className="font-mono">{installResult.missingDependencies.join(', ')}</span>
                  </span>
                </div>
              )}
            </div>
          )}
        </div>
      </Section>

      <Section
        title={<span className="flex items-center gap-2">Browse the Workshop</span>}
        description="Search the Steam Workshop for Conan Exiles mods and install them with one click."
        icon={<Search />}
        bodyClassName="py-4"
      >
        <div className="space-y-3">
          <div className="flex flex-wrap items-center gap-2">
            <Input
              value={searchText}
              onChange={(e) => setSearchText(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && doSearch()}
              placeholder="Search mods, e.g. “base building”, “graphics”, “QoL”…"
              className="max-w-md flex-1"
            />
            <Button
              size="sm"
              variant="primary"
              icon={<Search className="size-3.5" />}
              disabled={!searchText.trim()}
              loading={searchBusy}
              onClick={doSearch}
            >
              Search
            </Button>
            <Checkbox checked={enhancedOnly} onChange={setEnhancedOnly} label="Enhanced only" />
          </div>

          {!settings?.steamApiKey && (
            <Callout tone="info">
              Workshop search needs a Steam Web API key. Add one in <span className="font-medium">Settings → Steam Web API key</span> (free at steamcommunity.com/dev/apikey). You can still paste ids above without a key.
            </Callout>
          )}

          {searchResults &&
            (searchResults.length === 0 ? (
              <div className="rounded-lg border border-line bg-panel-2/50 p-4 text-center text-[12px] text-fg-4">
                No results. Try different keywords or turn off “Enhanced only”.
              </div>
            ) : (
              <div className="max-h-80 divide-y divide-line overflow-y-auto rounded-lg border border-line">
                {searchResults.map((item) => {
                  const added = addedIds.has(item.id);
                  return (
                    <div key={item.id} className="flex items-center gap-3 p-3">
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <span className="truncate text-[13px] font-medium text-fg">{item.title || `Mod ${item.id}`}</span>
                          {item.forConan && <Badge tone="ok">Conan</Badge>}
                        </div>
                        <div className="mt-0.5 text-[11.5px] text-fg-4">
                          {item.creator ? `by ${item.creator} · ` : ''}
                          {bytes(item.fileSize)} · updated {dateTime(item.timeUpdated * 1000)}
                        </div>
                      </div>
                      <Button
                        size="sm"
                        variant={added ? 'success' : 'primary'}
                        icon={added ? <CheckCircle2 className="size-3.5" /> : <Download className="size-3.5" />}
                        disabled={running || added}
                        loading={addingId === item.id}
                        onClick={() => addResult(item)}
                      >
                        {added ? 'Added' : 'Add'}
                      </Button>
                    </div>
                  );
                })}
              </div>
            ))}
        </div>
      </Section>

      {data && data.folderExists && enabled.length === 0 && (
        <Callout tone="warn" title="Empty ~mods folder">
          The server flags itself as modded whenever the ~mods folder exists – even when it's empty.{' '}
          <button className="text-accent hover:underline" disabled={running} onClick={() => run('clean', () => api.cleanStrayMods(id), 'Removed the empty mods folder')}>
            Remove it
          </button>
        </Callout>
      )}

      <Section
        title={<span className="flex items-center gap-2">Installed mods <Badge>{data?.mods.length ?? 0}</Badge></span>}
        icon={<Puzzle />}
        actions={
          updates.length > 0 ? (
            <Button
              size="sm"
              icon={<RefreshCw className="size-3.5" />}
              disabled={running}
              loading={busy === 'update'}
              onClick={() => run('update', () => api.updateMods(id, false), `Updating ${updates.length} mod${updates.length > 1 ? 's' : ''} — applied on next start`)}
            >
              Update {updates.length}
            </Button>
          ) : undefined
        }
        bodyClassName="py-1"
      >
        {!data ? (
          <div className="py-8 text-center text-[12px] text-fg-4">Loading…</div>
        ) : data.mods.length === 0 ? (
          <Empty
            icon={<Puzzle />}
            title="No mods installed"
            description="Install from the Workshop above, or add a local .pak file. Mods that mention UE4SS, Lua or dwmapi.dll run on players' PCs only."
            action={
              <Button variant="primary" icon={<Upload className="size-3.5" />} disabled={running} onClick={addLocal}>
                Add local .pak
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
