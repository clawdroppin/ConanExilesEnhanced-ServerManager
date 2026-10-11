import { useCallback, useEffect, useState } from 'react';
import { Ban, Crown, LogIn, LogOut, RefreshCw, Shield, UserCheck, Users } from 'lucide-react';
import { api, errMsg } from '@/lib/ipc';
import { clock } from '@/lib/format';
import type { ListEntry } from '@/lib/types';
import { useApp, useInstance, useStatus } from '@/store/app';
import { Button } from '@/components/ui/Button';
import { confirm } from '@/components/ui/Dialog';
import { Badge, Callout, Empty, PageHeader, Section } from '@/components/ui/Surface';

const LIST_META: Record<string, { title: string; icon: typeof Users; description: string; removable: string | null }> = {
  KnownPlayerList: { title: 'Known players', icon: UserCheck, description: 'Everyone who has joined this server, as recorded by the server.', removable: null },
  AdminList: { title: 'Admins', icon: Shield, description: 'Players who entered the admin password in Server Management.', removable: 'Revoke' },
  BannedUserList: { title: 'Banned', icon: Ban, description: 'Banned by the owner or an admin in-game.', removable: 'Unban' },
};

const field = (e: ListEntry, re: RegExp) => e.fields.find(([k]) => re.test(k))?.[1];

/** Display name; ids-only entries (admins, bans) borrow the name from the known-player list. */
function entryName(e: ListEntry, known: Map<string, string>) {
  const id = field(e, /id/i);
  return field(e, /name/i) ?? (id && known.get(id)) ?? id ?? e.fields[0]?.[1] ?? e.raw;
}

export function Players({ id }: { id: string }) {
  const inst = useInstance(id)!;
  const st = useStatus(id);
  const events = useApp((s) => s.players[id]) ?? [];
  const toast = useApp((s) => s.toast);
  const [lists, setLists] = useState<ListEntry[] | null>(null);
  const running = st?.state === 'running' || st?.state === 'starting';
  const known = new Map<string, string>();
  for (const e of lists ?? []) {
    const pid = field(e, /id/i);
    const name = field(e, /name/i);
    if (pid && name) known.set(pid, name);
  }

  const load = useCallback(() => {
    api.playerLists(id).then(setLists).catch((e) => toast('error', errMsg(e)));
  }, [id, toast]);
  useEffect(load, [load]);

  const remove = async (e: ListEntry, verb: string) => {
    const r = await confirm({ title: `${verb} ${entryName(e, known)}?`, message: 'The entry is removed from DedicatedServer.ini. It takes effect when the server next starts.', confirm: verb });
    if (!r.ok) return;
    try {
      await api.removeListEntry(id, e.list, e.raw);
      toast('success', `${verb}: ${entryName(e, known)}`, id);
      load();
    } catch (err) {
      toast('error', errMsg(err), id);
    }
  };

  return (
    <div className="mx-auto max-w-[1100px] space-y-5 p-6">
      <PageHeader
        title="Players"
        description="Who's online right now, recent joins and leaves, and the admin, ban and known-player lists the server keeps."
        actions={
          <Button size="sm" icon={<RefreshCw className="size-3.5" />} onClick={load}>
            Refresh lists
          </Button>
        }
      />
      <div className="grid grid-cols-[1fr_380px] gap-4">
        <Section
          title={
            <span className="flex items-center gap-2">
              Online now <Badge tone={st?.players.length ? 'ok' : 'neutral'}>{running ? `${st?.players.length ?? 0} / ${inst.launch.maxPlayers}` : 'offline'}</Badge>
            </span>
          }
          description="Detected from the server log as players join and leave."
          icon={<Users />}
          bodyClassName="py-3"
        >
          {!running ? (
            <Empty title="Server offline" description="Start the server to track who's connected." className="py-8" />
          ) : st?.players.length ? (
            <div className="grid grid-cols-2 gap-2">
              {st.players.map((p) => (
                <div key={p.userId} className="flex items-center gap-2.5 rounded-lg border border-line bg-bg-raised px-3 py-2">
                  <div className="flex size-7 items-center justify-center rounded-full bg-accent-dim text-[12px] font-semibold text-accent">{(p.charName || p.playerName).slice(0, 1).toUpperCase()}</div>
                  <span className="truncate text-[12.5px] font-medium text-fg">{p.charName || p.playerName}</span>
                  <span className="ml-auto size-2 rounded-full bg-ok" />
                </div>
              ))}
            </div>
          ) : (
            <Empty title="Nobody online" description="Players show up here a few seconds after joining." className="py-8" />
          )}
        </Section>
        <Section title="Session activity" icon={<LogIn />} bodyClassName="py-2 max-h-[260px] overflow-y-auto">
          {events.length === 0 ? (
            <div className="py-6 text-center text-[12px] text-fg-4">No joins or leaves this session.</div>
          ) : (
            [...events].reverse().map((e, i) => (
              <div key={i} className="flex items-center gap-2.5 py-1.5 text-[12.5px]">
                {e.kind === 'join' ? <LogIn className="size-3.5 text-ok" /> : <LogOut className="size-3.5 text-fg-4" />}
                <span className="text-fg">{e.player.charName || e.player.playerName}</span>
                <span className="text-fg-3">{e.kind === 'join' ? 'joined' : 'left'}</span>
                <span className="ml-auto font-mono text-[11px] text-fg-4">{clock(e.ts)}</span>
              </div>
            ))
          )}
        </Section>
      </div>

      <Callout tone="info" title="Roles in Conan Exiles">
        <span className="inline-flex items-center gap-1">
          <Crown className="size-3.5 text-gold" /> The <b className="text-fg">owner</b>
        </span>{' '}
        is the player whose Player ID matches the Owner ID – they can ban and unban anyone. <b className="text-fg">Admins</b> are players who entered the admin password under Pause → Settings → Server Management; they can ban online players but not unban. Kicks and bans happen in-game; here you can review and lift them.
      </Callout>

      {running && <Callout tone="warn">The server rewrites these lists while it runs. Stop it to unban or revoke admins.</Callout>}

      {(['BannedUserList', 'AdminList', 'KnownPlayerList'] as const).map((key) => {
        const meta = LIST_META[key];
        const rows = (lists ?? []).filter((e) => e.list === key);
        return (
          <Section key={key} title={<span className="flex items-center gap-2">{meta.title} <Badge>{rows.length}</Badge></span>} description={meta.description} icon={<meta.icon />} bodyClassName="py-2">
            {rows.length === 0 ? (
              <div className="py-5 text-center text-[12px] text-fg-4">{lists ? 'Empty.' : 'Loading…'}</div>
            ) : (
              <div className="divide-y divide-line">
                {rows.map((e, i) => (
                  <div key={i} className="flex items-center gap-3 py-2.5">
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-[12.5px] font-medium text-fg">{entryName(e, known)}</div>
                      <div className="selectable mt-0.5 flex flex-wrap gap-x-3 font-mono text-[11px] text-fg-4">
                        {e.fields.map(([k, v]) => (
                          <span key={k}>
                            {k}=<span className="text-fg-3">{v}</span>
                          </span>
                        ))}
                      </div>
                    </div>
                    {meta.removable && (
                      <Button size="sm" variant="outline" disabled={running} onClick={() => remove(e, meta.removable!)}>
                        {meta.removable}
                      </Button>
                    )}
                  </div>
                ))}
              </div>
            )}
          </Section>
        );
      })}
    </div>
  );
}
