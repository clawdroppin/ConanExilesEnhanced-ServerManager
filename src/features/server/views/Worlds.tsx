import { useCallback, useEffect, useState } from 'react';
import { Database, FolderOpen, Globe2, History, RefreshCw, RotateCcw, Star, Trash2, Upload } from 'lucide-react';
import { api, errMsg } from '@/lib/ipc';
import { bytes, dateTime, ago } from '@/lib/format';
import { pickFile, pickSave, reveal } from '@/lib/shell';
import type { WorldSnapshot } from '@/lib/types';
import { useApp, useInstance, useStatus } from '@/store/app';
import { Button, IconButton } from '@/components/ui/Button';
import { confirm } from '@/components/ui/Dialog';
import { Badge, Callout, Card, Empty, PageHeader, Section } from '@/components/ui/Surface';

export function Worlds({ id }: { id: string }) {
  const inst = useInstance(id)!;
  const st = useStatus(id);
  const toast = useApp((s) => s.toast);
  const [worlds, setWorlds] = useState<WorldSnapshot[] | null>(null);
  const running = st?.state === 'running' || st?.state === 'starting' || st?.state === 'stopping';
  const savedDir = `${inst.installDir}\\ConanSandbox\\Saved`;

  const load = useCallback(() => {
    api.listWorlds(id).then(setWorlds).catch((e) => toast('error', errMsg(e)));
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

  const importWorld = async () => {
    const p = await pickFile('Choose a Conan Exiles world (game.db or game_0.db)', ['db'], 'World database');
    if (!p) return;
    const r = await confirm({
      title: 'Import this world?',
      message: 'It becomes the live world (game_0.db). Your current world is moved to the _quarantine folder first, so it can be recovered.',
      confirm: 'Import world',
    });
    if (r.ok) act(() => api.importWorld(id, p), 'World imported and set live');
  };

  const exportWorld = async (s: WorldSnapshot) => {
    const d = await pickSave('Export world', s.name, ['db'], 'World database');
    if (d) act(() => api.exportWorld(id, s.name, d), 'World exported');
  };

  const live = (worlds ?? []).find((w) => w.active);
  const snapshots = (worlds ?? []).filter((w) => !w.active);

  return (
    <div className="mx-auto max-w-[1100px] space-y-5 p-6">
      <PageHeader
        title="Worlds"
        description="Your world is a single database (game_0.db). The server keeps rolling snapshots (game_0_backup_N.db) on every start – restore any of them to roll back."
        actions={
          <>
            <Button size="sm" icon={<RefreshCw className="size-3.5" />} onClick={load}>
              Refresh
            </Button>
            <Button size="sm" icon={<FolderOpen className="size-3.5" />} onClick={() => reveal(savedDir)}>
              Open folder
            </Button>
            <Button size="sm" variant="primary" icon={<Upload className="size-3.5" />} disabled={running} onClick={importWorld}>
              Import world
            </Button>
          </>
        }
      />
      {running && (
        <Callout tone="warn">
          The server is running and keeps saving its world. Stop it to import, restore or reset worlds.
        </Callout>
      )}

      <Section title="Your world" icon={<Globe2 />} bodyClassName="py-2">
        {worlds === null ? (
          <div className="py-8 text-center text-[12px] text-fg-4">Loading…</div>
        ) : !live ? (
          <Empty
            icon={<Globe2 />}
            title="No world yet"
            description="The first start creates a new world (game_0.db). Or import a game_0.db from another server or a single-player save."
          />
        ) : (
          <div className="flex items-center gap-3 py-3">
            <div className="flex size-9 items-center justify-center rounded-lg bg-accent-dim text-accent">
              <Database className="size-4" />
            </div>
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <span className="truncate text-[13px] font-medium text-fg">{live.name}</span>
                <Badge tone="accent">
                  <Star /> Live
                </Badge>
              </div>
              <div className="mt-0.5 font-mono text-[11px] text-fg-4">
                {bytes(live.size)} · saved {ago(live.modified)} · {dateTime(live.modified)}
              </div>
            </div>
            <Button size="sm" variant="outline" icon={<Upload className="size-3.5" />} onClick={() => exportWorld(live)}>
              Export a copy
            </Button>
          </div>
        )}
      </Section>

      <Section
        title="Snapshots"
        description="Rolling backups the server writes on every start (newest first). Restoring one replaces the live world – the current one is quarantined first."
        icon={<History />}
        bodyClassName="py-2"
      >
        {worlds === null ? (
          <div className="py-6 text-center text-[12px] text-fg-4">Looking…</div>
        ) : snapshots.length === 0 ? (
          <div className="py-6 text-center text-[12px] text-fg-4">
            No snapshots yet. The server creates one (game_0_backup_1.db) on its first start.
          </div>
        ) : (
          <div className="divide-y divide-line">
            {snapshots.map((s) => (
              <div key={s.path} className="flex items-center gap-3 py-3">
                <div className="flex size-9 items-center justify-center rounded-lg bg-panel-2 text-fg-4">
                  <History className="size-4" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="truncate text-[13px] font-medium text-fg">{s.name}</div>
                  <div className="mt-0.5 font-mono text-[11px] text-fg-4">
                    {bytes(s.size)} · {ago(s.modified)} · {dateTime(s.modified)}
                  </div>
                </div>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={running}
                  onClick={async () => {
                    const r = await confirm({
                      title: `Restore ${s.name}?`,
                      message: 'The live world is quarantined first, then this snapshot becomes the live world. Players roll back to this point.',
                      confirm: 'Restore',
                      danger: true,
                    });
                    if (r.ok) act(() => api.restoreWorld(id, s.name), 'Snapshot restored as the live world');
                  }}
                >
                  Restore
                </Button>
                <IconButton label="Export a copy" onClick={() => exportWorld(s)}>
                  <Upload />
                </IconButton>
                <IconButton
                  label="Delete (moved to quarantine)"
                  disabled={running}
                  className="hover:text-err"
                  onClick={async () => {
                    const r = await confirm({
                      title: `Delete ${s.name}?`,
                      message: 'The snapshot is moved to the _quarantine folder, so it can be recovered.',
                      confirm: 'Delete',
                      danger: true,
                    });
                    if (r.ok) act(() => api.deleteWorld(id, s.name), 'Snapshot moved to quarantine');
                  }}
                >
                  <Trash2 />
                </IconButton>
              </div>
            ))}
          </div>
        )}
      </Section>

      <Card className="flex items-center gap-4 border-err/20 p-4">
        <div className="min-w-0 flex-1">
          <div className="text-[13px] font-semibold text-fg">Start over with a fresh world</div>
          <div className="mt-0.5 text-[12px] text-fg-3">
            Moves the live world and every snapshot into quarantine (recoverable). The next start generates a brand-new world.
          </div>
        </div>
        <Button
          variant="danger"
          icon={<RotateCcw className="size-3.5" />}
          disabled={running}
          onClick={async () => {
            const r = await confirm({
              title: 'Reset the world?',
              message: 'The live world and all snapshots are quarantined (recoverable). Players lose progress on this server.',
              confirm: 'Reset world',
              danger: true,
            });
            if (r.ok) act(() => api.resetWorld(id), 'World reset – a new one is created on next start');
          }}
        >
          Reset world
        </Button>
      </Card>
    </div>
  );
}
