import { useEffect, useState, type ComponentType } from 'react';
import { AnimatePresence, motion } from 'motion/react';
import { AlertTriangle, Clock, Loader2, RotateCw, Save, Undo2, Users } from 'lucide-react';
import { cn } from '@/lib/cn';
import { serverColor } from '@/lib/colors';
import { duration } from '@/lib/format';
import { pingerPort } from '@/lib/net';
import { useApp, useInstance, useStatus, type ViewId } from '@/store/app';
import { Button } from '@/components/ui/Button';
import { StatusPill } from '@/components/ui/Surface';
import { useServerActions } from '@/hooks/useServerActions';
import { ServerControls } from './ServerControls';
import { VIEWS } from './views';
import { dirtyViews, useDraft } from './draft';
import { Overview } from './views/Overview';
import { Performance } from './views/Performance';
import { Console } from './views/Console';
import { Players } from './views/Players';
import { Worlds } from './views/Worlds';
import { Backups } from './views/Backups';
import { Settings } from './views/Settings';
import { GameRules } from './views/GameRules';
import { Launch } from './views/Launch';
import { Automation } from './views/Automation';
import { ConfigFiles } from './views/ConfigFiles';
import { Health } from './views/Health';
import { Mods } from './views/Mods';
import { PortForwarding } from './views/PortForwarding';
import { Files } from './views/Files';

const COMPONENTS: Record<ViewId, ComponentType<{ id: string }>> = {
  overview: Overview,
  performance: Performance,
  console: Console,
  players: Players,
  worlds: Worlds,
  backups: Backups,
  settings: Settings,
  rules: GameRules,
  launch: Launch,
  automation: Automation,
  config: ConfigFiles,
  health: Health,
  mods: Mods,
  ports: PortForwarding,
  files: Files,
};

function Uptime({ since }: { since: number }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);
  return <>{duration((now - since) / 1000)}</>;
}

function SaveBar({ id }: { id: string }) {
  const { dirty, saving, save, reset } = useDraft(id);
  const st = useStatus(id);
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key.toLowerCase() === 's') {
        e.preventDefault();
        if (dirty) save();
      }
    };
    window.addEventListener('keydown', h);
    return () => window.removeEventListener('keydown', h);
  }, [dirty, save]);
  return (
    <AnimatePresence>
      {dirty && (
        <motion.div
          initial={{ y: 24, opacity: 0 }}
          animate={{ y: 0, opacity: 1 }}
          exit={{ y: 24, opacity: 0 }}
          transition={{ duration: 0.16 }}
          className="pointer-events-auto absolute bottom-5 left-1/2 z-20 flex -translate-x-1/2 items-center gap-3 rounded-2xl border border-accent/30 bg-panel-2/95 py-2 pr-2 pl-4 shadow-glow backdrop-blur"
        >
          <span className="text-[12.5px] text-fg-2">
            Unsaved changes
            {st?.state === 'running' && <span className="text-fg-4"> · applied on next restart</span>}
          </span>
          <Button variant="ghost" size="sm" icon={<Undo2 className="size-3.5" />} onClick={reset}>
            Discard
          </Button>
          <Button variant="primary" size="sm" icon={<Save className="size-3.5" />} loading={saving} onClick={save}>
            Save <span className="opacity-60">Ctrl+S</span>
          </Button>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

export function Workspace({ id }: { id: string }) {
  const inst = useInstance(id);
  const st = useStatus(id);
  const view = useApp((s) => s.views[id] ?? 'overview');
  const setView = useApp((s) => s.setView);
  const { draft, saved } = useDraft(id);
  const dirty = dirtyViews(saved, draft);
  const { run, pending } = useServerActions(id);
  if (!inst) return null;
  const View = COMPONENTS[view] ?? Overview;
  const groups = ['Monitor', 'World', 'Configure', 'Maintain'] as const;

  return (
    <div className="flex h-full min-h-0">
      <nav className="flex w-[200px] shrink-0 flex-col gap-4 overflow-y-auto border-r border-line bg-bg-raised/50 px-2.5 py-4">
        {groups.map((g) => (
          <div key={g}>
            <div className="px-2.5 pb-1.5 text-[10.5px] font-semibold tracking-[0.12em] text-fg-4 uppercase">{g}</div>
            {VIEWS.filter((v) => v.group === g).map((v) => (
              <button
                key={v.id}
                onClick={() => setView(id, v.id)}
                className={cn(
                  'relative flex h-8 w-full items-center gap-2.5 rounded-lg px-2.5 text-[12.5px] transition-colors duration-150',
                  view === v.id ? 'bg-hover font-medium text-fg' : 'text-fg-3 hover:bg-panel-2 hover:text-fg-2',
                )}
              >
                {view === v.id && <motion.span layoutId={`nav-${id}`} className="bg-ember absolute top-1.5 bottom-1.5 left-0 w-[2px] rounded-full" transition={{ duration: 0.16 }} />}
                <v.icon className={cn('size-4', view === v.id ? 'text-accent' : 'text-fg-4')} />
                {v.label}
                {dirty.has(v.id) && <span className="ml-auto size-1.5 rounded-full bg-accent" />}
                {v.id === 'players' && (st?.players.length ?? 0) > 0 && <span className="ml-auto font-mono text-[10.5px] text-ok">{st!.players.length}</span>}
              </button>
            ))}
          </div>
        ))}
      </nav>
      <div className="relative flex min-w-0 flex-1 flex-col">
        <header className="flex shrink-0 items-center gap-4 border-b border-line bg-bg-raised/40 px-6 py-3">
          <div className="h-9 w-1 rounded-full" style={{ background: serverColor(inst.color) }} />
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2.5">
              <h2 className="truncate text-[15.5px] font-semibold tracking-tight text-fg">{inst.name}</h2>
              <StatusPill state={st?.state} busy={st?.busy} />
              {st?.busy && st.state !== 'running' && st.state !== 'stopped' && <Loader2 className="size-3.5 animate-spin text-fg-3" />}
            </div>
            <div className="mt-0.5 flex items-center gap-3 text-[11.5px] text-fg-3">
              <span className="font-mono">
                {inst.identity?.serverName || '—'} · UDP {inst.launch.port}/{pingerPort(inst)} · query {inst.launch.queryPort}
              </span>
              {st?.startedAt && (st.state === 'running' || st.state === 'starting') && (
                <span className="flex items-center gap-1">
                  <Clock className="size-3" /> <Uptime since={st.startedAt} />
                </span>
              )}
              {st?.state === 'running' && (
                <span className="flex items-center gap-1">
                  <Users className="size-3" /> {st.players.length}/{inst.launch.maxPlayers}
                </span>
              )}
            </div>
          </div>
          {st?.pendingRestart && st.state === 'running' && (
            <Button variant="outline" size="sm" className="border-warn/40 text-warn" icon={<RotateCw className="size-3.5" />} loading={pending === 'restart'} onClick={() => run('restart')}>
              Restart to apply changes
            </Button>
          )}
          {st?.crashLoop && (
            <span className="flex items-center gap-1.5 rounded-lg border border-err/30 bg-err/10 px-2 py-1 text-[11.5px] text-err">
              <AlertTriangle className="size-3.5" /> Crash loop – auto-restart paused
            </span>
          )}
          <ServerControls id={id} />
        </header>
        <div className="relative min-h-0 flex-1">
          <AnimatePresence mode="wait" initial={false}>
            <motion.div
              key={view}
              className="absolute inset-0 overflow-y-auto"
              initial={{ opacity: 0, y: 4 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.12 }}
            >
              <View id={id} />
            </motion.div>
          </AnimatePresence>
          <SaveBar id={id} />
        </div>
      </div>
    </div>
  );
}
