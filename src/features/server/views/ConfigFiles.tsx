import { useCallback, useEffect, useMemo, useState } from 'react';
import CodeMirror from '@uiw/react-codemirror';
import { StreamLanguage, HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { properties } from '@codemirror/legacy-modes/mode/properties';
import { EditorView } from '@codemirror/view';
import { tags as t } from '@lezer/highlight';
import { AlertTriangle, FileCode2, FolderOpen, History, RotateCcw, Save } from 'lucide-react';
import { cn } from '@/lib/cn';
import { api, errMsg } from '@/lib/ipc';
import { validateIni } from '@/lib/ini';
import { ago, dateTime } from '@/lib/format';
import { reveal } from '@/lib/shell';
import type { ConfigFile, ConfigFileName } from '@/lib/types';
import { useApp, useStatus } from '@/store/app';
import { Button, IconButton } from '@/components/ui/Button';
import { Select } from '@/components/ui/Field';
import { Callout } from '@/components/ui/Surface';
import { confirm } from '@/components/ui/Dialog';

const FILES: { name: ConfigFileName; hint: string }[] = [
  { name: 'DedicatedServer.ini', hint: 'Server identity, passwords, admin/ban lists' },
  { name: 'Engine.ini', hint: 'Engine & network tuning' },
  { name: 'Game.ini', hint: 'Game session (MaxPlayers)' },
  { name: 'GameUserSettings.ini', hint: 'Engine user settings' },
];

const theme = EditorView.theme(
  {
    '&': { backgroundColor: 'transparent', color: '#e7e0d9' },
    '.cm-content': { caretColor: '#fb923c', padding: '12px 0' },
    '.cm-gutters': { backgroundColor: 'transparent', color: '#5c514a', border: 'none' },
    '.cm-activeLine': { backgroundColor: 'rgb(255 237 213 / 0.03)' },
    '.cm-activeLineGutter': { backgroundColor: 'transparent', color: '#b8ada4' },
    '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': { backgroundColor: 'rgb(249 115 22 / 0.25) !important' },
    '.cm-cursor': { borderLeftColor: '#fb923c' },
    '.cm-searchMatch': { backgroundColor: 'rgb(250 204 21 / 0.25)' },
    '.cm-panels': { backgroundColor: '#1b1613', color: '#e7e0d9' },
  },
  { dark: true },
);

const highlight = HighlightStyle.define([
  { tag: t.comment, color: '#5c514a', fontStyle: 'italic' },
  { tag: t.heading, color: '#fb923c', fontWeight: '600' },
  { tag: [t.definition(t.propertyName), t.propertyName, t.variableName], color: '#fcd34d' },
  { tag: t.string, color: '#d6cfc8' },
  { tag: t.atom, color: '#60a5fa' },
]);

const extensions = [StreamLanguage.define(properties), syntaxHighlighting(highlight), theme, EditorView.lineWrapping];

export function ConfigFiles({ id }: { id: string }) {
  const st = useStatus(id);
  const toast = useApp((s) => s.toast);
  const upsert = useApp((s) => s.upsertInstance);
  const [file, setFile] = useState<ConfigFileName>('DedicatedServer.ini');
  const [loaded, setLoaded] = useState<ConfigFile | null>(null);
  const [text, setText] = useState('');
  const [saving, setSaving] = useState(false);
  const [history, setHistory] = useState<[string, number][]>([]);
  const dirty = loaded != null && text !== loaded.text;

  const load = useCallback(
    async (f: ConfigFileName) => {
      try {
        const c = await api.readConfig(id, f);
        setLoaded(c);
        setText(c.text);
        api.configHistory(id, f).then(setHistory).catch(() => setHistory([]));
      } catch (e) {
        toast('error', errMsg(e));
      }
    },
    [id, toast],
  );
  useEffect(() => {
    load(file);
  }, [file, load]);

  const issues = useMemo(() => validateIni(text), [text]);
  const errors = issues.filter((i) => i.severity === 'error');

  const save = useCallback(async () => {
    setSaving(true);
    try {
      const r = await api.writeConfig(id, file, text);
      upsert(r.instance);
      toast('success', r.deferred ? `${file} will be written when the server stops` : `${file} saved`, id);
      await load(file);
    } catch (e) {
      toast('error', errMsg(e), id);
    } finally {
      setSaving(false);
    }
  }, [id, file, text, load, toast, upsert]);

  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key.toLowerCase() === 's' && dirty) {
        e.preventDefault();
        save();
      }
    };
    window.addEventListener('keydown', h);
    return () => window.removeEventListener('keydown', h);
  }, [dirty, save]);

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-end gap-3 border-b border-line px-4 pt-3">
        <div className="flex min-w-0 flex-1 gap-1 overflow-x-auto [scrollbar-width:none]">
        {FILES.map((f) => (
          <button
            key={f.name}
            title={f.hint}
            onClick={async () => {
              if (f.name === file) return;
              if (dirty && !(await confirm({ title: 'Discard unsaved changes?', message: `Your edits to ${file} will be lost.`, confirm: 'Discard', danger: true })).ok) return;
              setFile(f.name);
            }}
            className={cn(
              'relative -mb-px flex shrink-0 items-center gap-2 rounded-t-lg border border-transparent px-3.5 py-2 text-[12.5px] transition-colors',
              file === f.name ? 'border-line border-b-panel bg-panel text-fg' : 'text-fg-3 hover:text-fg-2',
            )}
          >
            <FileCode2 className={cn('size-3.5', file === f.name ? 'text-accent' : 'text-fg-4')} />
            {f.name}
          </button>
        ))}
        </div>
        <div className="flex shrink-0 items-center gap-1.5 pb-2">
          {history.length > 0 && (
            <Select
              value=""
              className="w-40"
              onChange={async (e) => {
                if (!e.target.value) return;
                try {
                  setText(await api.readTextFile(id, e.target.value));
                  toast('info', 'Loaded an earlier version – review it and Save to restore');
                } catch (err) {
                  toast('error', errMsg(err));
                }
              }}
            >
              <option value="">History ({history.length})</option>
              {history.map(([p, ts]) => (
                <option key={p} value={p}>
                  {dateTime(ts)} · {ago(ts)}
                </option>
              ))}
            </Select>
          )}
          <IconButton label="Open folder" onClick={() => loaded && reveal(loaded.path.replace(/[\\/][^\\/]+$/, ''))}>
            <FolderOpen />
          </IconButton>
          <Button size="sm" variant="ghost" icon={<RotateCcw className="size-3.5" />} disabled={!dirty} onClick={() => setText(loaded?.text ?? '')}>
            Revert
          </Button>
          <Button size="sm" variant="primary" icon={<Save className="size-3.5" />} disabled={!dirty} loading={saving} onClick={save}>
            Save
          </Button>
        </div>
      </div>
      <div className="space-y-2 px-4 pt-3">
        {st?.state === 'running' && (
          <Callout tone="warn">The server rewrites its config on shutdown. Saves made now are kept aside and written as soon as it stops, then applied on the next start.</Callout>
        )}
        {file === 'DedicatedServer.ini' && (
          <Callout tone="info">
            Server Settings writes OwnerId, ServerName, DefaultWorldName, passwords and PlatformPolicy here on every start – edits to those keys also update Server Settings. ServerGuid and the admin/ban lists are written by the server itself.
          </Callout>
        )}
        {loaded && !loaded.exists && <Callout tone="info">This file doesn’t exist yet – the server or the manager creates it on first start. Saving creates it now.</Callout>}
      </div>
      <div className="m-4 flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-line bg-bg/70">
        <div className="min-h-0 flex-1 overflow-auto">
          <CodeMirror value={text} onChange={setText} extensions={extensions} theme="none" basicSetup={{ foldGutter: false, highlightActiveLine: true }} height="100%" style={{ height: '100%' }} />
        </div>
        <div className="flex shrink-0 items-center gap-4 border-t border-line px-4 py-1.5 font-mono text-[10.5px] text-fg-4">
          <span className="truncate">{loaded?.path}</span>
          {issues.length > 0 ? (
            <span className={cn('ml-auto flex items-center gap-1.5', errors.length ? 'text-err' : 'text-warn')} title={issues.map((i) => `Line ${i.line}: ${i.message}`).join('\n')}>
              <AlertTriangle className="size-3" /> {issues.length} issue{issues.length > 1 ? 's' : ''} – line {issues[0].line}: {issues[0].message}
            </span>
          ) : (
            <span className="ml-auto text-ok">No issues</span>
          )}
          {history.length > 0 && (
            <span className="flex items-center gap-1">
              <History className="size-3" /> {history.length} versions kept
            </span>
          )}
        </div>
      </div>
    </div>
  );
}
