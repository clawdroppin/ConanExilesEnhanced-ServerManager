import { useCallback, useEffect, useState } from 'react';
import { Download, FileUp, FolderOpen, Globe2, Laptop, Pencil, RefreshCw, RotateCcw, Star, Trash2, Upload } from 'lucide-react';
import { api, errMsg } from '@/lib/ipc';
import { bytes, dateTime, ago } from '@/lib/format';
import { pickFile, pickSave, reveal } from '@/lib/shell';
import type { SaveFile } from '@/lib/types';
import { useApp, useInstance, useStatus } from '@/store/app';
import { Button, IconButton } from '@/components/ui/Button';
import { confirm, Dialog } from '@/components/ui/Dialog';
import { Input } from '@/components/ui/Field';
import { Badge, Callout, Card, Empty, PageHeader, Section } from '@/components/ui/Surface';

export function Worlds({ id }: { id: string }) {
  const inst = useInstance(id)!;
  const st = useStatus(id);
  const toast = useApp((s) => s.toast);
  const [saves, setSaves] = useState<SaveFile[] | null>(null);
  const [local, setLocal] = useState<SaveFile[] | null>(null);
  const [renaming, setRenaming] = useState<SaveFile | null>(null);
  const [newName, setNewName] = useState('');
  const running = st?.state === 'running' || st?.state === 'starting' || st?.state === 'stopping';
  const savesDir = `${inst.installDir}\\ConanSandbox\\Saved\\SaveGames`;

  const load = useCallback(() => {
    api.listSaves(id).then(setSaves).catch((e) => toast('error', errMsg(e)));
    api.listLocalSaves(id).then(setLocal).catch(() => setLocal([]));
  }, [id, toast]);
  useEffect(load, [load]);

  const act = async (fn: () => Promise<unknown>, ok: string) => {
    try {
      await fn();
      toast('success', ok, id);
      load();
    } catch (e) {
      toast('error', errMsg(e), id);
    }
  };

  const importFile = async (path?: string) => {
    const p = path ?? (await pickFile('Choose a Conan Exiles world (.sav)', ['sav'], 'World save'));
    if (!p) return;
    const r = await confirm({
      title: 'Import this world?',
      message: 'It is copied into the server and becomes the active world (the server loads the newest save). A backup of the current saves is made first.',
      confirm: 'Import world',
    });
    if (r.ok) act(() => api.importSave(id, p), 'World imported and set active');
  };

  const worlds = (saves ?? []).filter((s) => !s.name.includes('\\') && !s.name.includes('/'));
  const other = (saves ?? []).filter((s) => s.name.includes('\\') || s.name.includes('/'));

  return (
    <div className="mx-auto max-w-[1100px] space-y-5 p-6">
      <PageHeader
        title="Worlds"
        description="On start the server loads the newest .sav in its SaveGames folder – or creates a fresh world named after Default World Name when there is none."
        actions={
          <>
            <Button size="sm" icon={<RefreshCw className="size-3.5" />} onClick={load}>
              Refresh
            </Button>
            <Button size="sm" icon={<FolderOpen className="size-3.5" />} onClick={() => reveal(savesDir)}>
              Open folder
            </Button>
            <Button size="sm" variant="primary" icon={<FileUp className="size-3.5" />} disabled={running} onClick={() => importFile()}>
              Import world
            </Button>
          </>
        }
      />
      {running && <Callout tone="warn">The server is running and keeps saving its world. Stop it to import, switch, rename or delete worlds.</Callout>}

      <Section title="Server worlds" icon={<Globe2 />} bodyClassName="py-2">
        {saves === null ? (
          <div className="py-8 text-center text-[12px] text-fg-4">Loading…</div>
        ) : worlds.length === 0 ? (
          <Empty
            icon={<Globe2 />}
            title="No world yet"
            description={`The first start creates a new Standard world called “${inst.ini?.defaultWorldName ?? 'MyWorld'}”. Or import one you made in single-player.`}
          />
        ) : (
          <div className="divide-y divide-line">
            {worlds.map((s) => (
              <div key={s.path} className="flex items-center gap-3 py-3">
                <div className={`flex size-9 items-center justify-center rounded-lg ${s.active ? 'bg-accent-dim text-accent' : 'bg-panel-2 text-fg-4'}`}>
                  <Globe2 className="size-4" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-[13px] font-medium text-fg">{s.name.replace(/\.sav$/i, '')}</span>
                    {s.active && (
                      <Badge tone="accent">
                        <Star /> Active
                      </Badge>
                    )}
                  </div>
                  <div className="mt-0.5 font-mono text-[11px] text-fg-4">
                    {bytes(s.size)} · saved {ago(s.modified)} · {dateTime(s.modified)}
                  </div>
                </div>
                {!s.active && (
                  <Button size="sm" variant="outline" disabled={running} onClick={() => act(() => api.activateSave(id, s.name), `${s.name} will load on next start`)}>
                    Make active
                  </Button>
                )}
                <IconButton
                  label="Rename"
                  disabled={running}
                  onClick={() => {
                    setRenaming(s);
                    setNewName(s.name.replace(/\.sav$/i, ''));
                  }}
                >
                  <Pencil />
                </IconButton>
                <IconButton
                  label="Export a copy"
                  onClick={async () => {
                    const d = await pickSave('Export world', s.name, ['sav'], 'World save');
                    if (d) act(() => api.exportSave(id, s.name, d), 'World exported');
                  }}
                >
                  <Upload />
                </IconButton>
                <IconButton
                  label="Delete (moved to quarantine)"
                  disabled={running}
                  className="hover:text-err"
                  onClick={async () => {
                    const r = await confirm({ title: `Delete ${s.name}?`, message: 'The file is moved to the _quarantine folder next to SaveGames, so it can be recovered.', confirm: 'Delete', danger: true });
                    if (r.ok) act(() => api.deleteSave(id, s.name), 'World moved to quarantine');
                  }}
                >
                  <Trash2 />
                </IconButton>
              </div>
            ))}
          </div>
        )}
        {other.length > 0 && (
          <div className="border-t border-line py-3 text-[11.5px] text-fg-4">
            Plus {other.length} other save file{other.length === 1 ? '' : 's'} in subfolders (player/character data) – included in backups.
          </div>
        )}
      </Section>

      <Section
        title="Worlds on this PC"
        description="Single-player or co-op worlds from the game client (%LOCALAPPDATA%\ConanSandbox\Saved\SaveGames). Create a custom world in-game, play in it for a minute or two, exit with Esc, then bring it here."
        icon={<Laptop />}
        bodyClassName="py-2"
      >
        {local === null ? (
          <div className="py-6 text-center text-[12px] text-fg-4">Looking…</div>
        ) : local.length === 0 ? (
          <div className="py-6 text-center text-[12px] text-fg-4">No local Conan Exiles saves found on this PC.</div>
        ) : (
          <div className="divide-y divide-line">
            {local.map((s) => (
              <div key={s.path} className="flex items-center gap-3 py-2.5">
                <Laptop className="size-4 text-fg-4" />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-[12.5px] text-fg">{s.name}</div>
                  <div className="font-mono text-[11px] text-fg-4">
                    {bytes(s.size)} · {dateTime(s.modified)}
                  </div>
                </div>
                <Button size="sm" variant="outline" icon={<Download className="size-3.5" />} disabled={running} onClick={() => importFile(s.path)}>
                  Use on server
                </Button>
              </div>
            ))}
          </div>
        )}
      </Section>

      <Card className="flex items-center gap-4 border-err/20 p-4">
        <div className="min-w-0 flex-1">
          <div className="text-[13px] font-semibold text-fg">Start over with a fresh world</div>
          <div className="mt-0.5 text-[12px] text-fg-3">Moves every save into quarantine (after a backup). The next start generates a new world named “{inst.ini?.defaultWorldName ?? 'MyWorld'}”.</div>
        </div>
        <Button
          variant="danger"
          icon={<RotateCcw className="size-3.5" />}
          disabled={running}
          onClick={async () => {
            const r = await confirm({ title: 'Reset the world?', message: 'All current saves are backed up and moved to quarantine. Players lose progress on this server.', confirm: 'Reset world', danger: true });
            if (r.ok) act(() => api.resetWorld(id), 'World reset – a new one is created on next start');
          }}
        >
          Reset world
        </Button>
      </Card>

      <Dialog
        open={!!renaming}
        onClose={() => setRenaming(null)}
        title="Rename world file"
        width={420}
        footer={
          <>
            <Button variant="ghost" onClick={() => setRenaming(null)}>
              Cancel
            </Button>
            <Button
              variant="primary"
              onClick={() => {
                const r = renaming!;
                setRenaming(null);
                act(() => api.renameSave(id, r.name, newName), 'Renamed');
              }}
            >
              Rename
            </Button>
          </>
        }
      >
        <Input value={newName} onChange={(e) => setNewName(e.target.value)} autoFocus />
        <div className="mt-2 space-y-1.5 text-[11.5px] text-fg-4">
          <p>The file name is the world’s slot name – it’s what players search for in Worlds → Public.</p>
          <p className="text-warn">Funcom advises against renaming world saves; if the world misbehaves afterwards, restore the original name (or a backup).</p>
        </div>
      </Dialog>
    </div>
  );
}
