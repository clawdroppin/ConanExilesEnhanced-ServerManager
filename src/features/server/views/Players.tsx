import { useCallback, useEffect, useMemo, useState } from 'react';
import { Ban, Gavel, History, ListChecks, LogIn, LogOut, RefreshCw, Search, ShieldCheck, UserMinus, UserPlus, Users, UserX } from 'lucide-react';
import { api, errMsg } from '@/lib/ipc';
import { ago, clock } from '@/lib/format';
import type { Character, ListEntry, ModerationRow, OnlinePlayer } from '@/lib/types';
import { useApp, useInstance, useStatus } from '@/store/app';
import { Button } from '@/components/ui/Button';
import { confirm } from '@/components/ui/Dialog';
import { Input, Toggle } from '@/components/ui/Field';
import { Badge, Callout, Empty, PageHeader, Section } from '@/components/ui/Surface';

// Conan Exiles player management:
//  * online players  – RCON `listplayers` (polled by the supervisor) → kick / ban / whitelist
//  * characters      – read-only from the world database (game_0.db)
//  * ban list        – Saved/blacklist.txt; whitelist – Saved/whitelist.txt (`<id>[:<name>]` lines)
//    Running server → changes go over RCON (it owns the files); stopped → the files are edited.
//  * whitelist only applies with EnableWhitelist=True (Game Rules).

function Row({ title, sub, children }: { title: React.ReactNode; sub?: React.ReactNode; children?: React.ReactNode }) {
  return (
    <div className="flex items-center gap-3 py-2.5">
      <div className="min-w-0 flex-1">
        <div className="truncate text-[12.5px] font-medium text-fg">{title}</div>
        {sub && <div className="selectable mt-0.5 flex flex-wrap gap-x-3 font-mono text-[11px] text-fg-4">{sub}</div>}
      </div>
      <div className="flex shrink-0 items-center gap-1.5">{children}</div>
    </div>
  );
}

export function Players({ id }: { id: string }) {
  const inst = useInstance(id)!;
  const st = useStatus(id);
  const events = useApp((s) => s.players[id]) ?? [];
  const toast = useApp((s) => s.toast);
  const [lists, setLists] = useState<ListEntry[] | null>(null);
  const [chars, setChars] = useState<Character[] | null>(null);
  const [log, setLog] = useState<ModerationRow[]>([]);
  const [whitelistOn, setWhitelistOn] = useState<boolean | null>(null);
  const [q, setQ] = useState('');
  const [newBan, setNewBan] = useState('');
  const [newWhite, setNewWhite] = useState('');
  const running = st?.state === 'running' || st?.state === 'starting';

  const load = useCallback(() => {
    api.playerLists(id).then(setLists).catch((e) => toast('error', errMsg(e)));
    api.characters(id).then(setChars).catch(() => setChars([]));
    api.moderationLog(id).then(setLog).catch(() => setLog([]));
    api
      .readGameRules(id)
      .then((r) => {
        const v = Object.entries(r.values).find(([k]) => k.toLowerCase() === 'enablewhitelist')?.[1];
        setWhitelistOn(v ? v.toLowerCase() === 'true' : false);
      })
      .catch(() => setWhitelistOn(null));
  }, [id, toast]);
  useEffect(load, [load, st?.state]);

  const names = useMemo(() => {
    const m = new Map<string, string>();
    for (const c of chars ?? []) {
      if (c.user) m.set(c.user, c.name);
      if (c.platformId) m.set(c.platformId, c.name);
    }
    for (const p of st?.players ?? []) {
      m.set(p.userId, p.charName || p.playerName);
      m.set(p.platformId, p.charName || p.playerName);
    }
    return m;
  }, [chars, st?.players]);

  const bans = (lists ?? []).filter((e) => e.list === 'ban');
  const whites = (lists ?? []).filter((e) => e.list === 'whitelist');
  const banned = new Set(bans.map((b) => b.id.toLowerCase()));
  const whitelisted = new Set(whites.map((b) => b.id.toLowerCase()));

  const act = async (action: 'kick' | 'ban' | 'warn', p: OnlinePlayer) => {
    const who = p.charName || p.playerName;
    const r = await confirm({
      title: `${action === 'kick' ? 'Kick' : action === 'ban' ? 'Ban' : 'Warn'} ${who}?`,
      message: action === 'warn' ? 'Sends a server-wide message naming the player.' : action === 'ban' ? 'They are disconnected and can’t rejoin until unbanned.' : 'They are disconnected but can rejoin.',
      confirm: action === 'warn' ? 'Send' : action === 'kick' ? 'Kick' : 'Ban',
      danger: action !== 'warn',
    });
    if (!r.ok) return;
    try {
      const out = await api.playerAction(id, action, p.userId, who, '');
      toast('success', out || `${who}: ${action}`, id);
      load();
    } catch (e) {
      toast('error', errMsg(e), id);
    }
  };

  const edit = async (list: 'ban' | 'whitelist', add: boolean, playerId: string, name = '') => {
    const pid = playerId.trim();
    if (!pid) return;
    if (list === 'ban' && add && !(await confirm({ title: `Ban ${name || pid}?`, message: 'They can’t join until unbanned.', confirm: 'Ban', danger: true })).ok) return;
    try {
      const out = await api.editPlayerList(id, list, add, pid, name);
      toast('success', out, id);
      if (list === 'ban') setNewBan('');
      else setNewWhite('');
      load();
    } catch (e) {
      toast('error', errMsg(e), id);
    }
  };

  const toggleWhitelist = async (on: boolean) => {
    try {
      await api.applyGameRules(id, { EnableWhitelist: on ? 'True' : 'False' }, {}, []);
      setWhitelistOn(on);
      toast('success', on ? 'Whitelist enforced – only listed players can join' : 'Whitelist off – anyone can join', id);
    } catch (e) {
      toast('error', errMsg(e), id);
    }
  };

  const shownChars = (chars ?? []).filter((c) => {
    const t = q.trim().toLowerCase();
    return !t || c.name.toLowerCase().includes(t) || c.clan.toLowerCase().includes(t) || c.user.toLowerCase().includes(t) || c.platformId.includes(t);
  });

  return (
    <div className="mx-auto max-w-[1100px] space-y-5 p-6">
      <PageHeader
        title="Players"
        description="Who’s online, every character in the world, bans and the whitelist."
        actions={
          <Button size="sm" icon={<RefreshCw className="size-3.5" />} onClick={load}>
            Refresh
          </Button>
        }
      />

      <div className="grid grid-cols-[1fr_340px] gap-4">
        <Section
          title={
            <span className="flex items-center gap-2">
              Online now <Badge tone={st?.players.length ? 'ok' : 'neutral'}>{running ? `${st?.players.length ?? 0} / ${inst.launch.maxPlayers}` : 'offline'}</Badge>
            </span>
          }
          description={st?.rconOk ? 'Live from RCON listplayers.' : 'Needs RCON to list players and act on them.'}
          icon={<Users />}
          bodyClassName="divide-y divide-line"
        >
          {!running ? (
            <Empty title="Server offline" description="Start the server to see who’s connected." className="py-8" />
          ) : st?.players.length ? (
            st.players.map((p) => (
              <Row
                key={p.userId || p.idx}
                title={
                  <span className="flex items-center gap-2">
                    <span className="size-2 rounded-full bg-ok" /> {p.charName || p.playerName}
                    {p.charName && p.playerName && p.playerName !== p.charName && <span className="text-fg-4">({p.playerName})</span>}
                  </span>
                }
                sub={
                  <>
                    <span>user {p.userId}</span>
                    {p.platformId && <span>{p.platform || 'platform'} {p.platformId}</span>}
                  </>
                }
              >
                <Button size="xs" variant="ghost" onClick={() => void act('warn', p)}>
                  Warn
                </Button>
                <Button size="xs" variant="outline" icon={<LogOut className="size-3" />} onClick={() => void act('kick', p)}>
                  Kick
                </Button>
                <Button size="xs" variant="danger" icon={<Ban className="size-3" />} onClick={() => void act('ban', p)}>
                  Ban
                </Button>
              </Row>
            ))
          ) : (
            <Empty title="Nobody online" description="Players show up here within a few seconds of joining." className="py-8" />
          )}
        </Section>
        <Section title="Session activity" icon={<LogIn />} bodyClassName="py-2 max-h-[300px] overflow-y-auto">
          {events.length === 0 ? (
            <div className="py-6 text-center text-[12px] text-fg-4">No joins or leaves this session.</div>
          ) : (
            [...events].reverse().map((e, i) => (
              <div key={i} className="flex items-center gap-2.5 py-1.5 text-[12.5px]">
                {e.kind === 'join' ? <LogIn className="size-3.5 text-ok" /> : <LogOut className="size-3.5 text-fg-4" />}
                <span className="truncate text-fg">{e.player.charName || e.player.playerName}</span>
                <span className="text-fg-3">{e.kind === 'join' ? 'joined' : 'left'}</span>
                <span className="ml-auto font-mono text-[11px] text-fg-4">{clock(e.ts)}</span>
              </div>
            ))
          )}
        </Section>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <Section title={<span className="flex items-center gap-2">Banned <Badge>{bans.length}</Badge></span>} description="Saved/blacklist.txt" icon={<Gavel />} bodyClassName="divide-y divide-line">
          <div className="flex gap-2 py-3">
            <Input mono value={newBan} onChange={(e) => setNewBan(e.target.value)} placeholder="Steam / platform id or Funcom user id" onKeyDown={(e) => e.key === 'Enter' && void edit('ban', true, newBan)} />
            <Button size="md" variant="danger" icon={<UserX className="size-3.5" />} disabled={!newBan.trim()} onClick={() => void edit('ban', true, newBan)}>
              Ban
            </Button>
          </div>
          {running && <div className="py-2 text-[11.5px] text-fg-4">While the server runs it can only ban players it has seen; stop it to add other ids.</div>}
          {bans.length === 0 ? (
            <div className="py-5 text-center text-[12px] text-fg-4">{lists ? 'Nobody is banned.' : 'Loading…'}</div>
          ) : (
            bans.map((b) => (
              <Row key={b.id} title={b.name || names.get(b.id) || b.id} sub={<span>{b.id}</span>}>
                <Button size="xs" variant="outline" onClick={() => void edit('ban', false, b.id)}>
                  Unban
                </Button>
              </Row>
            ))
          )}
        </Section>

        <Section
          title={<span className="flex items-center gap-2">Whitelist <Badge>{whites.length}</Badge></span>}
          description="Saved/whitelist.txt"
          icon={<ListChecks />}
          actions={
            whitelistOn != null && (
              <label className="flex items-center gap-2 text-[12px] text-fg-3">
                Enforce <Toggle size="sm" checked={whitelistOn} onChange={(v) => void toggleWhitelist(v)} />
              </label>
            )
          }
          bodyClassName="divide-y divide-line"
        >
          <div className="flex gap-2 py-3">
            <Input mono value={newWhite} onChange={(e) => setNewWhite(e.target.value)} placeholder="Steam / platform id or Funcom user id" onKeyDown={(e) => e.key === 'Enter' && void edit('whitelist', true, newWhite)} />
            <Button size="md" icon={<UserPlus className="size-3.5" />} disabled={!newWhite.trim()} onClick={() => void edit('whitelist', true, newWhite)}>
              Add
            </Button>
          </div>
          {whitelistOn === false && whites.length > 0 && <div className="py-2 text-[11.5px] text-warn">The list isn’t enforced – turn on “Enforce” (EnableWhitelist) to keep everyone else out.</div>}
          {whites.length === 0 ? (
            <div className="py-5 text-center text-[12px] text-fg-4">{lists ? 'Empty.' : 'Loading…'}</div>
          ) : (
            whites.map((w) => (
              <Row key={w.id} title={w.name || names.get(w.id) || w.id} sub={<span>{w.id}</span>}>
                <Button size="xs" variant="outline" icon={<UserMinus className="size-3" />} onClick={() => void edit('whitelist', false, w.id)}>
                  Remove
                </Button>
              </Row>
            ))
          )}
        </Section>
      </div>

      <Section
        title={<span className="flex items-center gap-2">Characters <Badge>{chars?.length ?? 0}</Badge></span>}
        description="Everyone with a character in this world (read from game_0.db)."
        icon={<ShieldCheck />}
        actions={
          <div className="relative w-56">
            <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-fg-4" />
            <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Name, clan or id" className="pl-8" />
          </div>
        }
        bodyClassName="divide-y divide-line max-h-[420px] overflow-y-auto"
      >
        {chars == null ? (
          <div className="py-6 text-center text-[12px] text-fg-4">Loading…</div>
        ) : shownChars.length === 0 ? (
          <Empty title={chars.length ? 'No match' : 'No characters yet'} description={chars.length ? undefined : 'Characters appear after players join for the first time.'} className="py-8" />
        ) : (
          shownChars.map((c) => {
            const pid = c.platformId || c.user;
            const isBanned = banned.has(c.platformId.toLowerCase()) || banned.has(c.user.toLowerCase());
            const isWhite = whitelisted.has(c.platformId.toLowerCase()) || whitelisted.has(c.user.toLowerCase());
            return (
              <Row
                key={c.id}
                title={
                  <span className="flex items-center gap-2">
                    {c.online && <span className="size-2 rounded-full bg-ok" />}
                    {c.name} <span className="text-fg-4">lvl {c.level}</span>
                    {c.clan && <Badge>{c.clan}</Badge>}
                    {isBanned && <Badge tone="err">banned</Badge>}
                    {isWhite && <Badge tone="ok">whitelisted</Badge>}
                  </span>
                }
                sub={
                  <>
                    {c.lastOnline > 0 && <span>seen {ago(c.lastOnline * 1000)}</span>}
                    {c.platformId && <span>platform {c.platformId}</span>}
                    {c.user && <span>user {c.user}</span>}
                  </>
                }
              >
                {!isWhite && pid && (
                  <Button size="xs" variant="ghost" onClick={() => void edit('whitelist', true, pid, c.name)}>
                    Whitelist
                  </Button>
                )}
                {pid &&
                  (isBanned ? (
                    <Button size="xs" variant="outline" onClick={() => void edit('ban', false, pid)}>
                      Unban
                    </Button>
                  ) : (
                    <Button size="xs" variant="danger" onClick={() => void edit('ban', true, pid, c.name)}>
                      Ban
                    </Button>
                  ))}
              </Row>
            );
          })
        )}
      </Section>

      <Section title="Moderation log" icon={<History />} description="Kicks, bans and list changes made through this manager." bodyClassName="divide-y divide-line max-h-[300px] overflow-y-auto">
        {log.length === 0 ? (
          <div className="py-5 text-center text-[12px] text-fg-4">Nothing yet.</div>
        ) : (
          log.map((m) => (
            <Row key={m.id} title={`${m.action} · ${m.playerName || names.get(m.userId) || m.userId}`} sub={<><span>{m.userId}</span>{m.reason && <span>“{m.reason}”</span>}<span>{ago(m.ts)}</span></>} />
          ))
        )}
      </Section>

      {!st?.rconOk && running && <Callout tone="warn">RCON isn’t connected, so kicks, bans and live list changes can’t be sent. Check Server Settings → RCON.</Callout>}
    </div>
  );
}
