import { useEffect, useState } from 'react';
import { Archive, ArrowRight, CalendarClock, Check, CircleDashed, Copy, Cpu, Globe2, HardDrive, KeyRound, MemoryStick, Network, Search, Terminal, Users } from 'lucide-react';
import { api } from '@/lib/ipc';
import { cn } from '@/lib/cn';
import { ago, bytes, dateTime, duration, pct, until } from '@/lib/format';
import { lineLevel, useLogLines, useServerHistory } from '@/lib/live';
import { copy } from '@/lib/shell';
import { beaconPort, netInfo } from '@/lib/net';
import type { NetInfo, ServerStats } from '@/lib/types';
import { useApp, useInstance, useStatus } from '@/store/app';
import { Sparkline } from '@/components/charts/Charts';
import { Button } from '@/components/ui/Button';
import { Badge, Callout, Card, Stat } from '@/components/ui/Surface';
import { ServerControls } from '../ServerControls';
import { useSearchName } from '../useSearchName';

function CopyChip({ label, value, icon }: { label: string; value: string; icon: React.ReactNode }) {
  const [done, setDone] = useState(false);
  return (
    <button
      onClick={() => {
        copy(value);
        setDone(true);
        setTimeout(() => setDone(false), 1200);
      }}
      className="group flex min-w-0 flex-1 items-center gap-3 rounded-xl border border-line bg-bg-raised px-3.5 py-2.5 text-left transition-colors hover:border-line-strong"
    >
      <div className="text-fg-4 [&_svg]:size-4">{icon}</div>
      <div className="min-w-0 flex-1">
        <div className="text-[10.5px] font-medium tracking-wide text-fg-4 uppercase">{label}</div>
        <div className="selectable truncate font-mono text-[13px] text-fg">{value || '—'}</div>
      </div>
      {done ? <Check className="size-3.5 text-ok" /> : <Copy className="size-3.5 text-fg-4 opacity-0 transition-opacity group-hover:opacity-100" />}
    </button>
  );
}


export function Overview({ id }: { id: string }) {
  const searchName = useSearchName(id);
  const inst = useInstance(id)!;
  const st = useStatus(id);
  const hist = useServerHistory(id);
  const lines = useLogLines(id);
  const setView = useApp((s) => s.setView);
  const info = useApp((s) => s.info);
  const [net, setNet] = useState<NetInfo | null>(null);
  useEffect(() => {
    netInfo().then(setNet).catch(() => {});
  }, []);
  const [fwOk, setFwOk] = useState<boolean | null>(null);
  useEffect(() => {
    api.firewallState(id).then((f) => setFwOk((f.portsOk || f.programRule) && f.blockedBy.length === 0)).catch(() => setFwOk(null));
  }, [id, st?.installedBuild]);
  const [stats, setStats] = useState<ServerStats | null>(null);
  useEffect(() => {
    api.serverStats(id).then(setStats).catch(() => setStats(null));
  }, [id, st?.state]);

  const win = hist.slice(-120);
  const last = hist[hist.length - 1];
  const live = st?.state === 'running' || st?.state === 'starting';
  const state = st?.state ?? 'stopped';
  const missing = !inst.ini?.serverName || !inst.ini?.defaultWorldName || !inst.ini?.adminPassword;
  const memLimit = (info?.totalMemory ?? 0) || 1;

  const checklist = [
    { done: state !== 'notInstalled', label: 'Install the dedicated server files', go: 'files' as const },
    { done: !missing, label: 'Set server name, world name and admin password', go: 'settings' as const },
    { done: !!inst.ini?.ownerId, label: 'Add your Owner ID (Player ID from the in-game Settings menu)', go: 'settings' as const },
    { done: fwOk !== false, label: 'Allow the server through Windows Firewall', go: 'health' as const },
    { done: live, label: 'Start the server', go: null },
  ];
  const setupDone = checklist.every((c) => c.done);
  const recent = lines.slice(-10);

  return (
    <div className="mx-auto max-w-[1200px] space-y-5 p-6">
      {!setupDone && (
        <Card className="relative overflow-hidden p-5">
          <div className="ember-grid pointer-events-none absolute inset-0 opacity-70" />
          <div className="relative flex items-start gap-6">
            <div className="min-w-0 flex-1">
              <div className="text-[15px] font-semibold text-fg">Get {inst.name} online</div>
              <div className="mt-1 text-[12.5px] text-fg-3">A few steps and your world is ready for friends.</div>
              <div className="mt-4 space-y-2">
                {checklist.map((c, i) => (
                  <button
                    key={i}
                    disabled={c.done || !c.go}
                    onClick={() => c.go && setView(id, c.go)}
                    className={cn('flex w-full items-center gap-2.5 text-left text-[12.5px]', c.done ? 'text-fg-4 line-through' : 'text-fg-2 hover:text-fg')}
                  >
                    {c.done ? <Check className="size-4 text-ok" /> : <CircleDashed className="size-4 text-accent" />}
                    {c.label}
                    {!c.done && c.go && <ArrowRight className="size-3.5 text-fg-4" />}
                  </button>
                ))}
              </div>
            </div>
            <ServerControls id={id} size="lg" compact={false} />
          </div>
        </Card>
      )}

      {st?.state === 'crashed' && (
        <Callout
          tone="err"
          title={st.crashLoop ? 'The server keeps crashing' : 'The server stopped unexpectedly'}
          action={
            <Button size="sm" icon={<Terminal className="size-3.5" />} onClick={() => setView(id, 'console')}>
              Open console
            </Button>
          }
        >
          {st.crashLoop
            ? `It crashed ${st.crashCount} times in 15 minutes, so automatic restarts are paused. Check the console and Network & Health, then start it again.`
            : 'Check the console for the last log lines. Automatic restart is ' + (inst.automation.autoRestart ? 'on.' : 'off.')}
        </Callout>
      )}

      <Card className="p-5">
        <div className="mb-3 flex items-center justify-between">
          <div className="text-[13px] font-semibold text-fg">How players join</div>
          <Badge tone={inst.ini?.worldPassword ? 'warn' : 'ok'}>
            <KeyRound /> {inst.ini?.worldPassword ? 'Password protected' : 'Open to everyone'}
          </Badge>
        </div>
        <div className="flex gap-3">
          <CopyChip label="Search name" value={searchName} icon={<Search />} />
          {st?.session?.JoinCode && <CopyChip label="Invite code" value={st.session.JoinCode} icon={<KeyRound />} />}
          <CopyChip label="Direct IP" value={net?.publicIp ? `${net.publicIp}:${inst.launch.port}` : 'detecting…'} icon={<Globe2 />} />
          <CopyChip label="LAN IP" value={net?.local[0] ? `${inst.launch.multihome || net.local[0].ip}:${inst.launch.port}` : '—'} icon={<Network />} />
        </div>
        <div className="mt-3 text-[11.5px] text-fg-4">
          In-game: Worlds → Public → search the world name, use the Direct IP tab{st?.session?.JoinCode ? ', or enter the invite code' : ''}. Friends outside your network need UDP {inst.launch.port} + {beaconPort(inst)} forwarded on your router –{' '}
          <button className="text-accent hover:underline" onClick={() => setView(id, 'ports')}>
            port forwarding guide
          </button>
          .
          {st?.session?.BuildId && <span className="ml-1 font-mono text-fg-3">Game version CL {st.session.BuildId} – players need the same game build.</span>}
        </div>
      </Card>

      <div className="grid grid-cols-4 gap-4">
        <Stat label="CPU" icon={<Cpu />} value={live && last ? pct(last.cpu, 1) : '—'} sub={live ? `${info?.cores ?? '?'} cores available` : 'Server offline'}>
          <Sparkline className="mt-3" values={win.map((p) => p.cpu)} max={Math.max(10, ...win.map((p) => p.cpu)) * 1.2} />
        </Stat>
        <Stat label="Memory" icon={<MemoryStick />} value={live && last ? bytes(last.mem) : '—'} sub={live && last ? `${pct((last.mem / memLimit) * 100, 1)} of system` : `Recommended ${2 + inst.launch.maxPlayers} GB`}>
          <Sparkline className="mt-3" values={win.map((p) => p.mem)} fit />
        </Stat>
        <Stat label="Players" icon={<Users />} value={live ? `${st?.players.length ?? 0}/${inst.launch.maxPlayers}` : '—'} sub={st?.players.length ? st.players.join(', ') : 'Detected from the server log'}>
          <button onClick={() => setView(id, 'players')} className="mt-3 text-[11.5px] text-accent hover:underline">
            Players & bans →
          </button>
        </Stat>
        <Stat label="Disk I/O" icon={<HardDrive />} value={live && last ? `${bytes(last.writeBps)}/s` : '—'} sub={live && last ? `write · ${bytes(last.readBps)}/s read` : '—'}>
          <Sparkline className="mt-3" values={win.map((p) => p.writeBps)} />
        </Stat>
      </div>

      <div className="grid grid-cols-[1fr_340px] gap-4">
        <Card className="flex min-h-[260px] flex-col overflow-hidden">
          <div className="flex items-center justify-between border-b border-line px-4 py-2.5">
            <div className="flex items-center gap-2 text-[12.5px] font-semibold text-fg">
              <Terminal className="size-4 text-fg-3" /> Recent console
            </div>
            <button onClick={() => setView(id, 'console')} className="text-[11.5px] text-accent hover:underline">
              Open console →
            </button>
          </div>
          <div className="selectable min-h-0 flex-1 overflow-hidden bg-bg/60 px-4 py-2.5 font-mono text-[11.5px] leading-[1.7]">
            {recent.length === 0 ? (
              <div className="text-fg-4">{live ? 'Waiting for log output…' : 'No output yet – start the server to see its log here.'}</div>
            ) : (
              recent.map((l) => (
                <div
                  key={l.n}
                  className={cn(
                    'truncate',
                    l.source === 'manager' ? 'text-accent' : l.source === 'steamcmd' ? 'text-info' : { error: 'text-err', warn: 'text-warn', display: 'text-fg-2', log: 'text-fg-3' }[lineLevel(l.text)],
                  )}
                >
                  {l.text}
                </div>
              ))
            )}
          </div>
        </Card>
        <div className="space-y-4">
          <Card className="p-4">
            <div className="text-[11.5px] font-medium tracking-wide text-fg-3 uppercase">Version</div>
            <div className="mt-2 flex items-center justify-between">
              <span className="font-mono text-[13px] text-fg">{st?.installedBuild ? `Build ${st.installedBuild}` : state === 'notInstalled' ? 'Not installed' : 'Unknown build'}</span>
              {st?.installedBuild && st.latestBuild && st.latestBuild !== st.installedBuild ? (
                <Badge tone="accent">Update available</Badge>
              ) : st?.latestBuild ? (
                <Badge tone="ok">Up to date</Badge>
              ) : null}
            </div>
            {st?.engineVersion && <div className="mt-1 truncate font-mono text-[11px] text-fg-4" title={st.engineVersion}>{st.engineVersion}</div>}
          </Card>
          <Card className="p-4">
            <div className="flex items-center gap-2 text-[11.5px] font-medium tracking-wide text-fg-3 uppercase">
              <CalendarClock className="size-3.5" /> Next scheduled
            </div>
            <div className="mt-2 text-[13px] text-fg">
              {st?.nextSchedule ? (
                <>
                  <span className="capitalize">{st.nextSchedule.action}</span> <span className="text-fg-3">{until(st.nextSchedule.at)}</span>
                </>
              ) : (
                <button onClick={() => setView(id, 'automation')} className="text-fg-3 hover:text-accent">
                  Nothing scheduled – add a daily restart →
                </button>
              )}
            </div>
          </Card>
          <Card className="p-4">
            <div className="flex items-center gap-2 text-[11.5px] font-medium tracking-wide text-fg-3 uppercase">
              <Users className="size-3.5" /> Server stats
            </div>
            {stats && stats.totalSessions > 0 ? (
              <div className="mt-2 space-y-1.5 text-[12.5px]">
                <div className="flex items-center justify-between">
                  <span className="text-fg-3">Total playtime</span>
                  <span className="font-mono text-fg">{duration(stats.totalPlaytimeSecs)}</span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-fg-3">Longest session</span>
                  <span className="font-mono text-fg">{duration(stats.longestSessionSecs)}</span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-fg-3">Unique players</span>
                  <span className="font-mono text-fg">{stats.uniquePlayers}</span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-fg-3">Sessions</span>
                  <span className="font-mono text-fg">{stats.totalSessions}</span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-fg-3">First / last seen</span>
                  <span className="font-mono text-[11.5px] text-fg-2">{dateTime(stats.firstSeen)} – {dateTime(stats.lastSeen)}</span>
                </div>
              </div>
            ) : (
              <div className="mt-2 text-[12.5px] text-fg-4">No sessions recorded yet – stats appear once players have joined.</div>
            )}
          </Card>
          <Card className="p-4">
            <div className="flex items-center gap-2 text-[11.5px] font-medium tracking-wide text-fg-3 uppercase">
              <Archive className="size-3.5" /> Last backup
            </div>
            <div className="mt-2 flex items-center justify-between text-[13px] text-fg">
              {ago(st?.lastBackup)}
              <button onClick={() => setView(id, 'backups')} className="text-[11.5px] text-accent hover:underline">
                Backups →
              </button>
            </div>
            <div className="mt-1 text-[11.5px] text-fg-4">
              {inst.automation.backupIntervalMins > 0 ? `Every ${inst.automation.backupIntervalMins} min while running` : 'Interval backups off'}
            </div>
          </Card>
        </div>
      </div>
    </div>
  );
}
