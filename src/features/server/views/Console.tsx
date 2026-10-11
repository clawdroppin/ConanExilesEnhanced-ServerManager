import { useEffect, useMemo, useRef, useState } from 'react';
import { ArrowDownToLine, Copy, Eraser, FileText, FolderOpen, Search, Send, X } from 'lucide-react';
import { cn } from '@/lib/cn';
import { api, errMsg } from '@/lib/ipc';
import { bytes, clock, dateTime } from '@/lib/format';
import { clearLines, lineLevel, pushLines, seedLines, useLogLines, type LogLine } from '@/lib/live';
import { copy, reveal } from '@/lib/shell';
import type { LogFile } from '@/lib/types';
import { useApp, useInstance, useStatus } from '@/store/app';
import { IconButton } from '@/components/ui/Button';
import { Input, Segmented, Select } from '@/components/ui/Field';

const ROW = 20;

type Level = 'all' | 'warn' | 'error';

const levelClass = { error: 'text-err', warn: 'text-warn', display: 'text-fg-2', log: 'text-fg-3' } as const;

function VirtualLines({ lines, follow, onUserScroll, query }: { lines: LogLine[]; follow: boolean; onUserScroll: (atBottom: boolean) => void; query: string }) {
  const ref = useRef<HTMLDivElement>(null);
  const [scroll, setScroll] = useState(0);
  const [height, setHeight] = useState(600);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setHeight(el.clientHeight));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  useEffect(() => {
    if (follow && ref.current) ref.current.scrollTop = ref.current.scrollHeight;
  }, [lines.length, follow]);
  const start = Math.max(0, Math.floor(scroll / ROW) - 20);
  const end = Math.min(lines.length, Math.ceil((scroll + height) / ROW) + 20);
  const q = query.toLowerCase();
  return (
    <div
      ref={ref}
      className="selectable relative min-h-0 flex-1 overflow-auto bg-bg/70 font-mono text-[11.5px]"
      onScroll={(e) => {
        const el = e.currentTarget;
        setScroll(el.scrollTop);
        onUserScroll(el.scrollHeight - el.scrollTop - el.clientHeight < ROW * 2);
      }}
    >
      <div style={{ height: lines.length * ROW, position: 'relative' }}>
        {lines.slice(start, end).map((l, i) => {
          const lvl = lineLevel(l.text);
          const idx = q ? l.text.toLowerCase().indexOf(q) : -1;
          return (
            <div
              key={l.n}
              className={cn('absolute right-0 left-0 flex gap-3 px-4 whitespace-pre hover:bg-panel/60', l.source === 'manager' ? 'text-accent' : l.source === 'steamcmd' ? 'text-info' : levelClass[lvl])}
              style={{ top: (start + i) * ROW, height: ROW, lineHeight: `${ROW}px` }}
            >
              <span className="w-[62px] shrink-0 text-fg-4/80">{clock(l.ts)}</span>
              <span className="w-[54px] shrink-0 text-fg-4/70">{l.source === 'server' ? 'server' : l.source === 'manager' ? 'manager' : l.source === 'rcon' ? 'rcon' : 'steam'}</span>
              <span>
                {idx >= 0 ? (
                  <>
                    {l.text.slice(0, idx)}
                    <mark className="rounded-sm bg-accent/30 text-fg">{l.text.slice(idx, idx + q.length)}</mark>
                    {l.text.slice(idx + q.length)}
                  </>
                ) : (
                  l.text
                )}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

export function Console({ id }: { id: string }) {
  const inst = useInstance(id)!;
  const st = useStatus(id);
  const live = useLogLines(id);
  const [query, setQuery] = useState('');
  const [level, setLevel] = useState<Level>('all');
  const [sources, setSources] = useState<Record<string, boolean>>({ server: true, manager: true, steamcmd: true });
  const [follow, setFollow] = useState(true);
  const [files, setFiles] = useState<LogFile[]>([]);
  const [fileView, setFileView] = useState<{ name: string; lines: LogLine[] } | null>(null);
  const [cmd, setCmd] = useState('');
  const [cmdBusy, setCmdBusy] = useState(false);
  const [cmdHistory, setCmdHistory] = useState<string[]>([]);
  const [cmdHistIdx, setCmdHistIdx] = useState(-1);
  const toast = useApp((s) => s.toast);

  useEffect(() => {
    api.logTail(id, 1500).then((l) => seedLines(id, l)).catch(() => {});
    api.logFiles(id).then(setFiles).catch(() => {});
  }, [id]);

  const base = fileView?.lines ?? live;
  const filtered = useMemo(() => {
    const q = query.toLowerCase();
    return base.filter((l) => {
      if (!sources[l.source === 'server' || l.source === 'manager' ? l.source : 'steamcmd']) return false;
      if (level !== 'all') {
        const lv = lineLevel(l.text);
        if (level === 'error' && lv !== 'error') return false;
        if (level === 'warn' && lv !== 'error' && lv !== 'warn') return false;
      }
      return !q || l.text.toLowerCase().includes(q);
    });
  }, [base, query, level, sources]);

  const counts = useMemo(() => {
    let e = 0;
    let w = 0;
    for (const l of base) {
      const lv = lineLevel(l.text);
      if (lv === 'error') e++;
      else if (lv === 'warn') w++;
    }
    return { e, w };
  }, [base]);

  const openFile = async (path: string) => {
    if (!path) return setFileView(null);
    try {
      const text = await api.readTextFile(id, path);
      const f = files.find((x) => x.path === path);
      setFileView({ name: f?.name ?? path, lines: text.split(/\r?\n/).map((t, i) => ({ n: i, ts: f?.modified ?? Date.now(), source: 'server', text: t })) });
      setFollow(false);
    } catch (e) {
      toast('error', errMsg(e));
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-line px-4 py-2.5">
        <div className="relative w-72">
          <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-fg-4" />
          <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Filter lines…" className="pl-8" />
          {query && (
            <button onClick={() => setQuery('')} className="absolute top-1/2 right-2 -translate-y-1/2 text-fg-4 hover:text-fg">
              <X className="size-3.5" />
            </button>
          )}
        </div>
        <Segmented
          size="sm"
          value={level}
          onChange={setLevel}
          options={[
            { value: 'all', label: 'All' },
            { value: 'warn', label: <>Warnings <span className="font-mono text-warn">{counts.w + counts.e}</span></> },
            { value: 'error', label: <>Errors <span className="font-mono text-err">{counts.e}</span></> },
          ]}
        />
        <div className="flex items-center gap-1 pl-1">
          {(['server', 'manager', 'steamcmd'] as const).map((s) => (
            <button
              key={s}
              onClick={() => setSources({ ...sources, [s]: !sources[s] })}
              className={cn('h-6 rounded-md border px-2 text-[11px] font-medium', sources[s] ? 'border-line-strong bg-panel-2 text-fg-2' : 'border-transparent text-fg-4 line-through')}
            >
              {s === 'steamcmd' ? 'SteamCMD' : s[0].toUpperCase() + s.slice(1)}
            </button>
          ))}
        </div>
        <div className="ml-auto flex items-center gap-1">
          <Select value={fileView ? files.find((f) => f.name === fileView.name)?.path ?? '' : ''} onChange={(e) => openFile(e.target.value)} className="w-56">
            <option value="">Live console</option>
            {files.map((f) => (
              <option key={f.path} value={f.path}>
                {f.name} · {bytes(f.size)} · {dateTime(f.modified)}
              </option>
            ))}
          </Select>
          <IconButton label="Copy visible lines" onClick={() => copy(filtered.map((l) => l.text).join('\n')).then(() => toast('success', `Copied ${filtered.length} lines`))}>
            <Copy />
          </IconButton>
          <IconButton label="Clear console" disabled={!!fileView} onClick={() => clearLines(id)}>
            <Eraser />
          </IconButton>
          <IconButton label="Open logs folder" onClick={() => reveal(`${inst.installDir}\\ConanSandbox\\Saved\\Logs`)}>
            <FolderOpen />
          </IconButton>
          <IconButton label={follow ? 'Following new lines' : 'Follow new lines'} active={follow} onClick={() => setFollow(!follow)}>
            <ArrowDownToLine />
          </IconButton>
        </div>
      </div>
      {fileView && (
        <div className="flex items-center gap-2 border-b border-line bg-info/5 px-4 py-1.5 text-[11.5px] text-info">
          <FileText className="size-3.5" /> Viewing {fileView.name} ({fileView.lines.length.toLocaleString()} lines)
          <button className="ml-auto text-fg-3 hover:text-fg" onClick={() => setFileView(null)}>
            Back to live
          </button>
        </div>
      )}
      {filtered.length === 0 ? (
        <div className="flex flex-1 items-center justify-center bg-bg/70 text-[12.5px] text-fg-4">{base.length ? 'No lines match the filter.' : 'No output yet. Server log lines appear here as soon as it starts.'}</div>
      ) : (
        <VirtualLines lines={filtered} follow={follow} onUserScroll={(b) => b !== follow && !fileView && setFollow(b)} query={query} />
      )}
      {/* RCON command input */}
      <div className="flex shrink-0 items-center gap-2 border-t border-line bg-panel/40 px-4 py-2">
        <span className="font-mono text-[12px] text-accent select-none">&gt;</span>
        <input
          value={cmd}
          onChange={(e) => setCmd(e.target.value)}
          onKeyDown={async (e) => {
            if (e.key === 'Enter' && cmd.trim() && !cmdBusy) {
              e.preventDefault();
              const c = cmd.trim();
              setCmd('');
              setCmdBusy(true);
              setCmdHistory((h) => [c, ...h].slice(0, 50));
              setCmdHistIdx(-1);
              pushLines(id, 'manager', [`> ${c}`], Date.now());
              try {
                const resp = await api.rcon(id, c);
                const respLines = resp.split(/\r?\n/).filter(Boolean);
                pushLines(id, 'rcon', respLines.length ? respLines : ['(no output)'], Date.now());
              } catch (err) {
                pushLines(id, 'rcon', [errMsg(err)], Date.now());
              } finally {
                setCmdBusy(false);
              }
            } else if (e.key === 'ArrowUp') {
              e.preventDefault();
              if (cmdHistory.length === 0) return;
              const idx = cmdHistIdx < 0 ? 0 : Math.min(cmdHistIdx + 1, cmdHistory.length - 1);
              setCmdHistIdx(idx);
              setCmd(cmdHistory[idx]);
            } else if (e.key === 'ArrowDown') {
              e.preventDefault();
              if (cmdHistIdx < 0) return;
              const idx = cmdHistIdx - 1;
              if (idx < 0) { setCmdHistIdx(-1); setCmd(''); }
              else { setCmdHistIdx(idx); setCmd(cmdHistory[idx]); }
            }
          }}
          placeholder={st?.state === 'running' ? 'Type a console command and press Enter…' : 'Server must be running to send commands'}
          disabled={cmdBusy}
          className="h-8 flex-1 rounded-md border border-line bg-bg/80 px-3 font-mono text-[12px] text-fg placeholder:text-fg-4 focus:border-accent focus:outline-none"
        />
        <button
          onClick={async () => {
            if (!cmd.trim() || cmdBusy) return;
            const c = cmd.trim();
            setCmd('');
            setCmdBusy(true);
            setCmdHistory((h) => [c, ...h].slice(0, 50));
            setCmdHistIdx(-1);
            pushLines(id, 'manager', [`> ${c}`], Date.now());
            try {
              const resp = await api.rcon(id, c);
              const respLines = resp.split(/\r?\n/).filter(Boolean);
              pushLines(id, 'rcon', respLines.length ? respLines : ['(no output)'], Date.now());
            } catch (err) {
              pushLines(id, 'rcon', [errMsg(err)], Date.now());
            } finally {
              setCmdBusy(false);
            }
          }}
          disabled={!cmd.trim() || cmdBusy}
          className="flex h-8 items-center gap-1.5 rounded-md bg-accent px-3 text-[12px] font-medium text-white transition-opacity hover:opacity-90 disabled:opacity-40"
        >
          <Send className="size-3.5" /> Send
        </button>
      </div>
      <div className="flex shrink-0 items-center gap-4 border-t border-line px-4 py-1.5 font-mono text-[10.5px] text-fg-4">
        <span>{filtered.length.toLocaleString()} / {base.length.toLocaleString()} lines</span>
        <span>{inst.installDir}\ConanSandbox\Saved\Logs\ConanSandbox.log</span>
      </div>
    </div>
  );
}
