import { useCallback, useEffect, useMemo, useState } from 'react';
import { Check, Copy, Cpu, Gauge, RotateCcw, Rocket, Save, Terminal, TriangleAlert } from 'lucide-react';
import { cn } from '@/lib/cn';
import { api, errMsg } from '@/lib/ipc';
import { IniDoc } from '@/lib/ini';
import { copy } from '@/lib/shell';
import type { ConfigFileName } from '@/lib/types';
import { ENGINE_SETTINGS, LAUNCH_OPTIONS, PRIORITIES, type EngineSetting } from '@/data/catalog';
import { useApp, useStatus } from '@/store/app';
import { Button } from '@/components/ui/Button';
import { FieldRow, Input, NumberInput, Select, Toggle } from '@/components/ui/Field';
import { Badge, Callout, Card, PageHeader, Section } from '@/components/ui/Surface';
import { useDraft } from '../draft';

function LaunchPreview({ id }: { id: string }) {
  const { draft } = useDraft(id);
  const [line, setLine] = useState('');
  const [copied, setCopied] = useState(false);
  const key = JSON.stringify(draft?.launch);
  useEffect(() => {
    const t = setTimeout(() => {
      if (draft) api.previewLaunchLine(draft).then(setLine).catch(() => {});
    }, 150);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return (
    <Card className="overflow-hidden">
      <div className="flex items-center justify-between border-b border-line px-4 py-2.5">
        <div className="flex items-center gap-2 text-[12.5px] font-semibold text-fg">
          <Terminal className="size-4 text-fg-3" /> Launch command
        </div>
        <button
          onClick={() => {
            copy(line);
            setCopied(true);
            setTimeout(() => setCopied(false), 1200);
          }}
          className="flex items-center gap-1.5 text-[11.5px] text-fg-3 hover:text-fg"
        >
          {copied ? <Check className="size-3.5 text-ok" /> : <Copy className="size-3.5" />} Copy
        </button>
      </div>
      <div className="selectable bg-bg/70 px-4 py-3 font-mono text-[11.5px] leading-relaxed break-all text-fg-2">{line || '…'}</div>
      <div className="border-t border-line px-4 py-2 text-[11px] text-fg-4">Passwords and names never go on the command line – they’re written to DedicatedServer.ini.</div>
    </Card>
  );
}

function EngineTuning({ id }: { id: string }) {
  const toast = useApp((s) => s.toast);
  const st = useStatus(id);
  const [docs, setDocs] = useState<Record<string, string> | null>(null);
  const [edits, setEdits] = useState<Record<string, string | null>>({});
  const [saving, setSaving] = useState(false);

  const load = useCallback(async () => {
    const files = [...new Set(ENGINE_SETTINGS.map((s) => s.file))];
    const out: Record<string, string> = {};
    for (const f of files) out[f] = (await api.readConfig(id, f)).text;
    setDocs(out);
    setEdits({});
  }, [id]);
  useEffect(() => {
    load().catch((e) => toast('error', errMsg(e)));
  }, [load, toast]);

  const k = (s: EngineSetting) => `${s.file}|${s.section}|${s.key}`;
  const parsed = useMemo(() => Object.fromEntries(Object.entries(docs ?? {}).map(([f, t]) => [f, new IniDoc(t)])), [docs]);
  const current = (s: EngineSetting): string | undefined => (k(s) in edits ? (edits[k(s)] ?? undefined) : parsed[s.file]?.get(s.section, s.key));
  const dirty = Object.keys(edits).length > 0;

  const save = async () => {
    if (!docs) return;
    setSaving(true);
    try {
      const byFile: Record<string, IniDoc> = {};
      for (const s of ENGINE_SETTINGS) {
        if (!(k(s) in edits)) continue;
        const doc = (byFile[s.file] ??= new IniDoc(docs[s.file] ?? ''));
        const v = edits[k(s)];
        if (v == null) doc.remove(s.section, s.key);
        else doc.set(s.section, s.key, v);
      }
      let deferred = false;
      for (const [f, doc] of Object.entries(byFile)) {
        const r = await api.writeConfig(id, f as ConfigFileName, doc.toString());
        deferred ||= r.deferred;
      }
      toast('success', deferred ? 'Saved – written when the server stops, applied on next start' : 'Engine settings saved', id);
      await load();
    } catch (e) {
      toast('error', errMsg(e), id);
    } finally {
      setSaving(false);
    }
  };

  const groups = ['Networking', 'Tick & frame rate', 'Logging'] as const;
  return (
    <Section
      title="Engine tuning"
      description="Standard Unreal Engine settings written to Engine.ini. Funcom doesn’t document these for Conan Exiles, so everything starts unset (engine default) – change one thing at a time."
      icon={<Gauge />}
      actions={
        <>
          {dirty && (
            <Button size="sm" variant="ghost" onClick={() => setEdits({})}>
              Discard
            </Button>
          )}
          <Button size="sm" variant={dirty ? 'primary' : 'secondary'} icon={<Save className="size-3.5" />} disabled={!dirty} loading={saving} onClick={save}>
            Save engine settings
          </Button>
        </>
      }
      bodyClassName="pb-2"
    >
      {st?.state === 'running' && <Callout tone="info" className="mt-4">Changes are written once the server stops and apply on its next start.</Callout>}
      {groups.map((g) => (
        <div key={g}>
          <div className="pt-4 pb-1 text-[11px] font-semibold tracking-[0.1em] text-fg-4 uppercase">{g}</div>
          <div className="divide-y divide-line">
            {ENGINE_SETTINGS.filter((s) => s.group === g).map((s) => {
              const v = current(s);
              const isSet = v !== undefined;
              return (
                <FieldRow
                  key={k(s)}
                  label={s.label}
                  description={
                    <>
                      {s.description}
                      <span className="mt-0.5 block font-mono text-[10.5px] text-fg-4">
                        [{s.section}] {s.key} · default {s.engineDefault}
                      </span>
                    </>
                  }
                  badge={isSet ? <Badge tone="accent">Custom</Badge> : undefined}
                >
                  <div className="flex items-center gap-2">
                    <div className={cn('flex-1', !isSet && 'opacity-60')}>
                      {s.type === 'bool' ? (
                        <Toggle checked={(v ?? s.engineDefault).toLowerCase() === 'true'} onChange={(b) => setEdits({ ...edits, [k(s)]: b ? 'True' : 'False' })} />
                      ) : s.type === 'enum' ? (
                        <Select value={v ?? s.engineDefault} onChange={(e) => setEdits({ ...edits, [k(s)]: e.target.value })}>
                          {s.options!.map((o) => (
                            <option key={o}>{o}</option>
                          ))}
                        </Select>
                      ) : (
                        <NumberInput
                          value={+(v ?? s.engineDefault) || 0}
                          min={s.min}
                          max={s.max}
                          step={s.step ?? 1}
                          suffix={s.suffix}
                          onChange={(n) => setEdits({ ...edits, [k(s)]: s.type === 'int' ? String(Math.round(n)) : String(n) })}
                        />
                      )}
                    </div>
                    <button
                      title="Reset to engine default"
                      disabled={!isSet}
                      onClick={() => setEdits({ ...edits, [k(s)]: null })}
                      className="flex size-7 items-center justify-center rounded-md text-fg-4 hover:bg-hover hover:text-fg disabled:opacity-0"
                    >
                      <RotateCcw className="size-3.5" />
                    </button>
                  </div>
                </FieldRow>
              );
            })}
          </div>
        </div>
      ))}
    </Section>
  );
}

export function Launch({ id }: { id: string }) {
  const { draft, patch } = useDraft(id);
  const info = useApp((s) => s.info);
  if (!draft) return null;
  const l = draft.launch;
  const cores = info?.cores ?? 8;
  const optVal = (key: string, def?: boolean) => l.options[key] ?? def ?? false;

  return (
    <div className="mx-auto max-w-[1000px] space-y-5 p-6 pb-24">
      <PageHeader title="Launch & Engine" description="Command-line switches, process priority and CPU cores, plus engine-level tuning." />
      <LaunchPreview id={id} />

      <Section title="Launch switches" icon={<Rocket />} description="Passed to the server executable on start." bodyClassName="divide-y divide-line">
        {LAUNCH_OPTIONS.map((o) => {
          const v = optVal(o.key, o.default);
          return (
            <FieldRow
              key={o.key}
              label={
                <span className="flex items-center gap-2">
                  {o.label}
                  <code className="rounded bg-panel-2 px-1.5 py-0.5 font-mono text-[10.5px] text-fg-3">{o.key}</code>
                </span>
              }
              badge={o.tag ? <Badge tone={o.tag === 'Funcom' ? 'accent' : o.tag === 'Recommended' ? 'ok' : 'neutral'}>{o.tag}</Badge> : undefined}
              description={o.description}
              error={o.warnOff && v === false ? o.warnOff : undefined}
            >
              {o.kind === 'flag' ? (
                <Toggle checked={v === true} onChange={(b) => patch((d) => void (d.launch.options[o.key] = b))} />
              ) : (
                <Input
                  value={typeof v === 'string' ? v : ''}
                  placeholder={o.placeholder}
                  onChange={(e) =>
                    patch((d) => {
                      if (e.target.value) d.launch.options[o.key] = e.target.value;
                      else delete d.launch.options[o.key];
                    })
                  }
                />
              )}
            </FieldRow>
          );
        })}
        <FieldRow label="Extra arguments" description="Anything else, appended as-is. Quote values with spaces. -Port / -QueryPort / -MULTIHOME here are ignored – set them under Server Settings." stacked>
          <Input mono value={l.extraArgs} placeholder='-ini:Engine:[ConsoleVariables]:net.MaxPlayersOverride=6' onChange={(e) => patch((d) => void (d.launch.extraArgs = e.target.value))} />
        </FieldRow>
      </Section>

      <Section title="Process" icon={<Cpu />} description="Priority and affinity apply immediately to a running server, too." bodyClassName="divide-y divide-line">
        <FieldRow label="Show console window" description="The engine console is hidden by default. Turn on to keep it visible on the desktop.">
          <Toggle checked={l.showConsole} onChange={(b) => patch((d) => void (d.launch.showConsole = b))} />
        </FieldRow>
        <FieldRow label="CPU priority" description="Above normal can help on a busy PC that also runs games. High may starve other apps.">
          <Select value={l.priority} onChange={(e) => patch((d) => void (d.launch.priority = e.target.value))}>
            {PRIORITIES.map((p) => (
              <option key={p.value} value={p.value}>
                {p.label}
              </option>
            ))}
          </Select>
        </FieldRow>
        <FieldRow
          label="CPU cores"
          description="Restrict the server to specific cores – e.g. keep it off the cores your game uses. None selected means all cores."
          stacked
          hint={l.affinity.length ? `${l.affinity.length} of ${cores} cores` : `All ${cores} cores`}
        >
          <div className="flex flex-wrap gap-1.5">
            {Array.from({ length: cores }, (_, c) => {
              const on = l.affinity.includes(c);
              return (
                <button
                  key={c}
                  onClick={() => patch((d) => void (d.launch.affinity = on ? d.launch.affinity.filter((x) => x !== c) : [...d.launch.affinity, c].sort((a, b) => a - b)))}
                  className={cn(
                    'h-7 w-9 rounded-md border font-mono text-[11px] transition-colors',
                    on ? 'border-accent-strong bg-accent/15 text-accent' : 'border-line-strong bg-bg-raised text-fg-3 hover:text-fg',
                  )}
                >
                  {c}
                </button>
              );
            })}
            {l.affinity.length > 0 && (
              <Button size="sm" variant="ghost" onClick={() => patch((d) => void (d.launch.affinity = []))}>
                Use all
              </Button>
            )}
          </div>
          {l.affinity.length > 0 && l.affinity.length < 2 && (
            <div className="mt-2 flex items-center gap-1.5 text-[11.5px] text-warn">
              <TriangleAlert className="size-3.5" /> One core is very little for an Unreal server.
            </div>
          )}
        </FieldRow>
      </Section>

      <EngineTuning id={id} />
    </div>
  );
}
