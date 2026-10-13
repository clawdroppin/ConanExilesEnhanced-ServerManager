import { useCallback, useEffect, useMemo, useState } from 'react';
import { Check, Info, RefreshCw, RotateCcw, Search, Sparkles, Undo2 } from 'lucide-react';
import { cn } from '@/lib/cn';
import { api, errMsg } from '@/lib/ipc';
import type { GameRules as Rules } from '@/lib/types';
import { GAME_RULES, RULE_GROUPS, type GameRule } from '@/data/gameRules';
import { useApp, useStatus } from '@/store/app';
import { Button, IconButton } from '@/components/ui/Button';
import { Input, NumberInput, Select, Toggle } from '@/components/ui/Field';
import { Badge, Callout, Empty, PageHeader, Section } from '@/components/ui/Surface';
import { confirm } from '@/components/ui/Dialog';

// Game rules = ServerSettings.ini [ServerSettings]. The FILE is the source of truth: the server
// writes it itself when an admin changes settings in-game (and on a clean shutdown), so this view
// always re-reads it and only sends the keys you changed. Running server → applied live over RCON
// (enum indexes are translated to names by the backend); stopped → written into the file.

const MANAGED = new Set(['adminpassword', 'serverpassword', 'servermessageoftheday', 'servermodlist', 'configversion', 'blueprintconfigversion']);
const KNOWN = new Set(GAME_RULES.map((r) => r.key.toLowerCase()));

/** Quick starting points – they only stage values; nothing is written until you apply. */
const PRESETS: { id: string; label: string; hint: string; values: Record<string, string> }[] = [
  {
    id: 'relaxed',
    label: 'Relaxed PvE',
    hint: 'Keep your gear on death, faster progress, no decay.',
    values: { PVPEnabled: 'False', DropEquipmentOnDeath: '0', HarvestAmountMultiplier: '2', PlayerXPRateMultiplier: '2', ItemConvertionMultiplier: '0.5', ThrallConversionMultiplier: '0.5', DisableBuildingAbandonment: 'True', DisableThrallDecay: 'True', DurabilityMultiplier: '0.5' },
  },
  {
    id: 'pvp',
    label: 'Weekend raids',
    hint: 'PvP on, building damage only Fri–Sun 18:00–23:00.',
    values: {
      PVPEnabled: 'True', CanDamagePlayerOwnedStructures: 'True', RestrictPVPBuildingDamageTime: 'True', DropEquipmentOnDeath: '1',
      ...Object.fromEntries(['Friday', 'Saturday', 'Sunday'].flatMap((d) => [[`PVPBuildingDamageEnabled${d}`, 'True'], [`PVPBuildingDamageTime${d}Start`, '1800'], [`PVPBuildingDamageTime${d}End`, '2300']])),
    },
  },
  {
    id: 'small',
    label: 'Small group boost',
    hint: 'For 1–4 friends: 3× harvest & XP, cheaper crafting, lighter hunger.',
    values: { HarvestAmountMultiplier: '3', PlayerXPRateMultiplier: '3', CraftingCostMultiplier: '0.5', PlayerActiveHungerMultiplier: '0.5', PlayerActiveThirstMultiplier: '0.5', PlayerEncumbranceMultiplier: '2', ResourceRespawnSpeedMultiplier: '2' },
  },
];

function isNumeric(r: GameRule) {
  return r.type === 'int' || r.type === 'float' || r.type === 'hhmm';
}

function sameValue(r: GameRule | undefined, a: string, b: string) {
  if (r && isNumeric(r) && a.trim() !== '' && b.trim() !== '' && !isNaN(+a) && !isNaN(+b)) return Math.abs(+a - +b) < 1e-4;
  if (r?.type === 'bool') return a.toLowerCase() === b.toLowerCase();
  return a === b;
}

function pretty(r: GameRule, v: string) {
  if (v === '') return '—';
  if (r.type === 'enum') return r.options?.find((o) => o.value === v.trim())?.label ?? v;
  if (r.type === 'float' && !isNaN(+v)) return String(+(+v).toFixed(4));
  if (r.type === 'hhmm') return hhmm(v);
  return v + (r.suffix && r.suffix !== '×' ? ` ${r.suffix}` : r.suffix === '×' ? '×' : '');
}

function hhmm(v: string) {
  const n = parseInt(v, 10);
  if (isNaN(n)) return v;
  return `${String(Math.floor(n / 100)).padStart(2, '0')}:${String(n % 100).padStart(2, '0')}`;
}

function RuleControl({ rule, value, onChange }: { rule: GameRule; value: string; onChange: (v: string) => void }) {
  switch (rule.type) {
    case 'bool':
      return <Toggle checked={value.toLowerCase() === 'true' || value === '1'} onChange={(v) => onChange(v ? 'True' : 'False')} />;
    case 'enum':
      return (
        <Select value={rule.options?.some((o) => o.value === value.trim()) ? value.trim() : ''} onChange={(e) => onChange(e.target.value)}>
          {!rule.options?.some((o) => o.value === value.trim()) && <option value="">{value ? `Custom: ${value}` : 'Game default'}</option>}
          {rule.options?.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </Select>
      );
    case 'int':
    case 'float':
      return (
        <NumberInput
          value={value.trim() === '' || isNaN(+value) ? 0 : +(+value).toFixed(4)}
          step={rule.step ?? 1}
          min={rule.min}
          suffix={rule.suffix}
          onChange={(v) => onChange(String(rule.type === 'int' ? Math.round(v) : v))}
        />
      );
    case 'hhmm':
      return (
        <Input
          type="time"
          value={value.trim() === '' ? '' : hhmm(value)}
          onChange={(e) => {
            const [h, m] = e.target.value.split(':').map((x) => parseInt(x, 10));
            onChange(isNaN(h) ? '0' : String(h * 100 + (m || 0)));
          }}
        />
      );
    default:
      return <Input mono value={value} placeholder="(empty)" onChange={(e) => onChange(e.target.value)} />;
  }
}

export function GameRules({ id }: { id: string }) {
  const st = useStatus(id);
  const toast = useApp((s) => s.toast);
  const [rules, setRules] = useState<Rules | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [staged, setStaged] = useState<Record<string, string>>({});
  const [query, setQuery] = useState('');
  const [group, setGroup] = useState<string>('all');
  const [changedOnly, setChangedOnly] = useState(false);
  const [busy, setBusy] = useState(false);
  const running = st?.state === 'running' || st?.state === 'starting';

  const load = useCallback(async () => {
    try {
      setRules(await api.readGameRules(id));
      setError(null);
    } catch (e) {
      setError(errMsg(e));
    }
  }, [id]);
  // Re-read whenever the server changes state: an admin may have changed rules in-game, and a
  // clean shutdown rewrites the file from the server's memory.
  useEffect(() => {
    void load();
  }, [load, st?.state]);

  /** Case-insensitive view of the file (the server writes e.g. `serverVoiceChat`). */
  const file = useMemo(() => {
    const m = new Map<string, { key: string; value: string }>();
    for (const [k, v] of Object.entries(rules?.values ?? {})) m.set(k.toLowerCase(), { key: k, value: v });
    for (const [k, v] of Object.entries(rules?.pending ?? {})) m.set(k.toLowerCase(), { key: k, value: v });
    return m;
  }, [rules]);

  const current = (r: GameRule) => file.get(r.key.toLowerCase())?.value ?? r.def;
  const effective = (r: GameRule) => staged[r.key] ?? current(r);
  const isSetInFile = (r: GameRule) => file.has(r.key.toLowerCase());
  const differsFromDefault = (r: GameRule) => r.def !== '' && !sameValue(r, effective(r), r.def);

  const other = useMemo(
    () => [...file.entries()].filter(([k]) => !KNOWN.has(k) && !MANAGED.has(k)).map(([, v]) => v),
    [file],
  );

  const q = query.trim().toLowerCase();
  const visible = GAME_RULES.filter(
    (r) =>
      (group === 'all' || r.group === group) &&
      (!q || r.key.toLowerCase().includes(q) || r.label.toLowerCase().includes(q) || (r.desc ?? '').toLowerCase().includes(q)) &&
      (!changedOnly || differsFromDefault(r) || r.key in staged),
  );
  const counts = useMemo(() => {
    const c: Record<string, number> = {};
    for (const r of GAME_RULES) if (r.def !== '' && !sameValue(r, file.get(r.key.toLowerCase())?.value ?? r.def, r.def)) c[r.group] = (c[r.group] ?? 0) + 1;
    return c;
  }, [file]);

  const stage = (key: string, v: string) => {
    const r = GAME_RULES.find((x) => x.key === key);
    setStaged((s) => {
      const next = { ...s };
      const base = r ? current(r) : (file.get(key.toLowerCase())?.value ?? '');
      if (sameValue(r, v, base)) delete next[key];
      else next[key] = v;
      return next;
    });
  };
  const stagedKeys = Object.keys(staged);

  const apply = async () => {
    if (!stagedKeys.length) return;
    setBusy(true);
    try {
      // Keep the key spelling the file already uses (e.g. serverVoiceChat).
      const changes: Record<string, string> = {};
      for (const [k, v] of Object.entries(staged)) changes[file.get(k.toLowerCase())?.key ?? k] = v;
      const restartKeys = GAME_RULES.filter((r) => r.restart).map((r) => r.key);
      const res = await api.applyGameRules(id, changes, {}, restartKeys);
      const parts = [
        res.live.length && `${res.live.length} applied live`,
        res.written.length && `${res.written.length} saved`,
        res.queued.length && `${res.queued.length} queued until the server stops`,
      ].filter(Boolean);
      if (res.failed.length) toast('error', `Not applied: ${res.failed.map(([k, why]) => `${k} (${why})`).join(', ')}`, id);
      if (parts.length) toast('success', `Game rules: ${parts.join(', ')}${res.restartNeeded ? ' – some take effect after a restart' : ''}`, id);
      setStaged({});
      await load();
    } catch (e) {
      toast('error', errMsg(e), id);
    } finally {
      setBusy(false);
    }
  };

  const resetToDefault = async (r: GameRule) => {
    if (running) {
      // Can't delete a key from a running server – set the default value instead.
      if (r.def !== '') stage(r.key, r.def);
      return;
    }
    try {
      await api.resetGameRules(id, [file.get(r.key.toLowerCase())?.key ?? r.key]);
      setStaged((s) => {
        const n = { ...s };
        delete n[r.key];
        return n;
      });
      await load();
    } catch (e) {
      toast('error', errMsg(e), id);
    }
  };

  const usePreset = async (p: (typeof PRESETS)[number]) => {
    if (stagedKeys.length && !(await confirm({ title: `Stage “${p.label}”?`, message: 'It is added on top of your unapplied changes. Nothing is written until you click Apply.', confirm: 'Stage' })).ok) return;
    for (const [k, v] of Object.entries(p.values)) stage(k, v);
    toast('info', `${Object.keys(p.values).length} rules staged from “${p.label}” – review and Apply.`, id);
  };

  if (error && !rules) return <Empty title="Couldn’t read ServerSettings.ini" description={error} className="py-20" />;

  return (
    <div className="mx-auto max-w-[1100px] space-y-5 p-6 pb-28">
      <PageHeader
        title="Game Rules"
        description="Every rule in ServerSettings.ini – PvP, loot, rates, decay, storms, thralls and more. While the server runs, changes apply live; otherwise they’re saved to the file."
        actions={
          <Button size="sm" icon={<RefreshCw className="size-3.5" />} onClick={() => void load()}>
            Re-read file
          </Button>
        }
      />

      <Callout tone="info" title="Changed something in-game as admin?">
        That’s fine – the server saves admin changes into ServerSettings.ini and this page always shows the file. The manager never resets rules on start; it only writes the keys you change here.
      </Callout>

      {rules && !rules.exists && <Callout tone="warn">ServerSettings.ini doesn’t exist yet. Values you apply now create it; the server fills in the rest on its first start.</Callout>}
      {rules && Object.keys(rules.pending ?? {}).length > 0 && (
        <Callout tone="warn">
          {Object.keys(rules.pending ?? {}).length} change(s) couldn’t be sent live (RCON unavailable) and will be written as soon as the server stops: {Object.keys(rules.pending ?? {}).join(', ')}.
        </Callout>
      )}

      <Section title="Quick presets" icon={<Sparkles />} description="Stage a set of common values, review them below, then Apply." bodyClassName="flex flex-wrap gap-2 py-3">
        {PRESETS.map((p) => (
          <button key={p.id} onClick={() => void usePreset(p)} className="rounded-lg border border-line-strong bg-bg-raised px-3 py-2 text-left hover:border-accent/60">
            <div className="text-[12.5px] font-medium text-fg">{p.label}</div>
            <div className="text-[11.5px] text-fg-4">{p.hint}</div>
          </button>
        ))}
      </Section>

      <div className="sticky top-0 z-10 -mx-2 space-y-2 bg-bg/95 px-2 py-2 backdrop-blur">
        <div className="flex items-center gap-2">
          <div className="relative flex-1">
            <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-fg-4" />
            <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search 290 rules – e.g. loot, harvest, decay, raid…" className="pl-8" />
          </div>
          <label className="flex shrink-0 items-center gap-2 text-[12px] text-fg-3">
            <Toggle size="sm" checked={changedOnly} onChange={setChangedOnly} /> Changed from default
          </label>
        </div>
        <div className="flex flex-wrap gap-1.5">
          {[{ id: 'all', label: 'All' }, ...RULE_GROUPS].map((g) => (
            <button
              key={g.id}
              onClick={() => setGroup(g.id)}
              className={cn('rounded-full border px-2.5 py-0.5 text-[11.5px]', group === g.id ? 'border-accent/60 bg-accent/10 text-accent' : 'border-line-strong text-fg-3 hover:text-fg')}
            >
              {g.label}
              {g.id !== 'all' && counts[g.id] ? <span className="ml-1 text-fg-4">{counts[g.id]}</span> : null}
            </button>
          ))}
        </div>
      </div>

      {RULE_GROUPS.filter((g) => visible.some((r) => r.group === g.id)).map((g) => (
        <Section key={g.id} title={g.label} bodyClassName="divide-y divide-line">
          {visible
            .filter((r) => r.group === g.id)
            .map((r) => {
              const v = effective(r);
              const isStaged = r.key in staged;
              const changed = differsFromDefault(r);
              const unmet = (r.requires ?? []).filter((dep) => {
                const neg = dep.startsWith('!');
                const d = GAME_RULES.find((x) => x.key === dep.replace('!', ''));
                if (!d || d.type !== 'bool') return false;
                const on = effective(d).toLowerCase() === 'true';
                return neg ? on : !on;
              });
              return (
                <div key={r.key} className={cn('grid grid-cols-[minmax(0,1fr)_minmax(220px,300px)_28px] items-start gap-x-6 py-3', isStaged && 'bg-accent/[0.04]')}>
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-1.5 text-[13px] font-medium text-fg">
                      {r.label}
                      {isStaged && <Badge tone="accent">unsaved</Badge>}
                      {!isStaged && changed && <Badge tone="info">custom</Badge>}
                      {r.restart && <Badge tone="warn">restart</Badge>}
                      {r.undocumented && <Badge tone="violet">undocumented</Badge>}
                    </div>
                    {r.desc && <div className="mt-0.5 text-[12px] leading-relaxed text-fg-3">{r.desc}</div>}
                    <div className="mt-0.5 flex flex-wrap gap-x-3 font-mono text-[10.5px] text-fg-4">
                      <span className="selectable">{r.key}</span>
                      {r.def !== '' && <span>default {pretty(r, r.def)}</span>}
                      {!isSetInFile(r) && <span>not in file (game default)</span>}
                    </div>
                    {unmet.length > 0 && (
                      <div className="mt-1 flex items-center gap-1 text-[11px] text-warn">
                        <Info className="size-3" /> Needs {unmet.map((u) => (u.startsWith('!') ? `${u.slice(1)} off` : `${u} on`)).join(', ')} to have an effect.
                      </div>
                    )}
                  </div>
                  <div className={cn('min-w-0', r.type === 'bool' && 'flex justify-end pt-0.5')}>
                    <RuleControl rule={r} value={v} onChange={(nv) => stage(r.key, nv)} />
                  </div>
                  <div className="pt-1">
                    {isStaged ? (
                      <IconButton label="Undo" onClick={() => setStaged((s) => { const n = { ...s }; delete n[r.key]; return n; })}>
                        <Undo2 />
                      </IconButton>
                    ) : (changed || isSetInFile(r)) && r.def !== '' ? (
                      <IconButton label={running ? 'Set back to default' : 'Remove from file (use the game default)'} onClick={() => void resetToDefault(r)}>
                        <RotateCcw />
                      </IconButton>
                    ) : null}
                  </div>
                </div>
              );
            })}
        </Section>
      ))}

      {other.length > 0 && (group === 'all' || group === 'tech') && (
        <Section title="Other keys in the file" description="Keys the catalog doesn’t know – added by mods, newer game versions or by hand. Edited as plain text." bodyClassName="divide-y divide-line">
          {other
            .filter((o) => !q || o.key.toLowerCase().includes(q))
            .map((o) => (
              <div key={o.key} className="grid grid-cols-[minmax(0,1fr)_minmax(220px,300px)_28px] items-center gap-x-6 py-2.5">
                <span className="selectable truncate font-mono text-[12px] text-fg-2">{o.key}</span>
                <Input mono value={staged[o.key] ?? o.value} onChange={(e) => stage(o.key, e.target.value)} />
                <span />
              </div>
            ))}
        </Section>
      )}

      {visible.length === 0 && <Empty title="No rules match" description="Try another search or turn off “Changed from default”." className="py-10" />}

      {stagedKeys.length > 0 && (
        <div className="sticky bottom-4 z-20 ml-auto flex w-fit items-center gap-3 rounded-xl border border-accent/40 bg-panel px-4 py-2.5 shadow-2xl">
          <span className="text-[12.5px] text-fg-2">
            {stagedKeys.length} unsaved change{stagedKeys.length > 1 ? 's' : ''} · {running ? 'applies live over RCON' : 'saved to ServerSettings.ini'}
          </span>
          <Button size="sm" variant="ghost" onClick={() => setStaged({})}>
            Discard
          </Button>
          <Button size="sm" variant="primary" icon={<Check className="size-3.5" />} loading={busy} onClick={() => void apply()}>
            Apply
          </Button>
        </div>
      )}
    </div>
  );
}
