import { useEffect, useState } from 'react';
import { Crown, Gamepad2, Globe2, IdCard, Map as MapIcon, Network, Palette, ScrollText, TerminalSquare, Wand2 } from 'lucide-react';
import { cn } from '@/lib/cn';
import { api } from '@/lib/ipc';
import { netInfo, pingerPort } from '@/lib/net';
import { useMaps } from '@/lib/useMaps';
import { SERVER_COLORS } from '@/lib/colors';
import type { NetInfo } from '@/lib/types';
import { useApp, useStatus } from '@/store/app';
import { Button } from '@/components/ui/Button';
import { Checkbox, FieldRow, Input, NumberInput, SecretInput, Select, Slider, TextArea, Toggle } from '@/components/ui/Field';
import { Badge, Callout, PageHeader, Section } from '@/components/ui/Surface';
import { useDraft } from '../draft';

// Server Settings = the values this profile owns and writes on every start (docs/DESIGN.md §5):
//   ServerSettings.ini  AdminPassword, ServerPassword, ServerMessageOfTheDay   (+ ServerModList, Mods view)
//   Engine.ini          [OnlineSubsystem] ServerName/ServerPassword, [Regions] ExcludedRegions, ServerDefaultMap
//   Game.ini            [RconPlugin]
//   command line        -Port -QueryPort -MaxPlayers -MULTIHOME -MULTIHOMEHTTP
// Values an admin changes in-game (MOTD, passwords) are adopted back into the profile by the
// backend (server::adopt_server_changes) instead of being overwritten. Gameplay rules live in
// Game Rules (ServerSettings.ini is their source of truth).

const REGIONS = [
  { id: 'ExiledLands', label: 'Exiled Lands', hint: 'The main map’s regions' },
  { id: 'IsleOfSiptah', label: 'Isle of Siptah', hint: 'Siptah DLC area – skipping it saves RAM if nobody owns the DLC' },
];

function Len({ v, max }: { v: string; max: number }) {
  return <span className={cn('font-mono text-[10.5px]', v.length > max ? 'text-warn' : 'text-fg-4')}>{v.length}/{max}</span>;
}

export function Settings({ id }: { id: string }) {
  const { draft, patch } = useDraft(id);
  const instances = useApp((s) => s.instances);
  const setView = useApp((s) => s.setView);
  const st = useStatus(id);
  const running = st?.state === 'running' || st?.state === 'starting';
  const [net, setNet] = useState<NetInfo | null>(null);
  const maps = useMaps(id);
  useEffect(() => {
    netInfo().then(setNet).catch(() => {});
  }, []);
  if (!draft) return null;
  const ident = draft.identity;
  const l = draft.launch;
  const r = draft.rcon;
  const mine = [l.port, pingerPort(draft), l.queryPort];
  const clash = instances.filter((o) => o.id !== id && [o.launch.port, pingerPort(o), o.launch.queryPort].some((p) => p > 0 && mine.includes(p)));
  const rconClash = instances.find((o) => o.id !== id && o.rcon.enabled && r.enabled && o.rcon.port === r.port);
  const queryErr = l.queryPort === l.port || l.queryPort === pingerPort(draft) ? `Must differ from the game port and game port + 1 (${pingerPort(draft)}).` : undefined;
  const mapValue = draft.ini?.defaultMap ?? '';
  // If the profile holds a map we don't list (e.g. a mod map), keep it selectable.
  const mapOptions = maps.some((m) => m.value === mapValue) ? maps : [{ value: mapValue, label: 'Custom map', hint: mapValue }, ...maps];

  return (
    <div className="mx-auto max-w-[1000px] space-y-5 p-6 pb-24">
      <PageHeader
        title="Server Settings"
        description="Name, passwords, map, ports and RCON – written into the server’s config files on every start. Gameplay rules (PvP, loot on death, rates…) are under Game Rules."
      />

      <Callout
        tone="info"
        title="Admin changes made in-game are kept"
        action={
          <Button size="sm" icon={<ScrollText className="size-3.5" />} onClick={() => setView(id, 'rules')}>
            Game Rules
          </Button>
        }
      >
        If you change the message of the day or a password from inside the game as admin, the manager takes your in-game value into this profile on the next stop/start instead of overwriting it. Gameplay rules you change in-game are never touched.
      </Callout>

      <Section title="Listing" description="What players see in the in-game server browser." icon={<Globe2 />} bodyClassName="divide-y divide-line">
        <FieldRow
          label="Server name"
          description="Shown in the server browser (Engine.ini ServerName). Players search for it."
          badge={<Badge tone="err">Required</Badge>}
          hint={<Len v={ident.serverName} max={64} />}
          error={!ident.serverName.trim() ? 'The server won’t start without a name.' : undefined}
        >
          <Input value={ident.serverName} invalid={!ident.serverName.trim()} onChange={(e) => patch((d) => void (d.identity.serverName = e.target.value))} />
        </FieldRow>
        <FieldRow label="Message of the day" description="Shown to players when they join. Updates live while the server runs – no restart needed." stacked>
          <TextArea value={ident.motd} rows={2} placeholder="Welcome, exile! Be nice, build big." onChange={(e) => patch((d) => void (d.identity.motd = e.target.value))} />
        </FieldRow>
        <FieldRow label="Join password" description="Players need it to join. Leave empty for a public server.">
          <SecretInput value={ident.serverPassword} onChange={(v) => patch((d) => void (d.identity.serverPassword = v))} placeholder="Open to everyone" generate />
        </FieldRow>
        <FieldRow label="Max players" description="Player cap. Plan on roughly 2 GB RAM + 1 GB per 10 players for a vanilla world (mods add more)." hint={`Recommended RAM: ~${Math.ceil(4 + l.maxPlayers / 10)} GB`}>
          <div className="flex items-center gap-3">
            <Slider value={l.maxPlayers} min={1} max={200} onChange={(v) => patch((d) => void (d.launch.maxPlayers = v))} />
            <input
              type="number"
              min={1}
              max={200}
              value={l.maxPlayers}
              onChange={(e) => patch((d) => void (d.launch.maxPlayers = Math.max(1, Math.min(200, Number(e.target.value) || 1))))}
              className="h-8 w-16 rounded-md border border-line bg-bg px-2 text-center font-mono text-[13px] text-fg focus:border-accent focus:outline-none"
            />
          </div>
        </FieldRow>
      </Section>

      <Section title="World" icon={<MapIcon />} bodyClassName="divide-y divide-line">
        <FieldRow label="Map" description="Engine.ini ServerDefaultMap. Each map keeps its own world, so switching starts a fresh world on that map. Mod maps show up once the mod is installed.">
          <Select value={mapValue} onChange={(e) => patch((d) => void (d.ini = { ...d.ini, defaultMap: e.target.value }))}>
            {mapOptions.map((m) => (
              <option key={m.value} value={m.value}>
                {m.label} — {m.hint}
              </option>
            ))}
          </Select>
        </FieldRow>
        <FieldRow label="Skip map regions" description="Engine.ini [Regions] ExcludedRegions – regions that aren’t loaded at all. Saves memory; players can’t enter them.">
          <div className="space-y-1.5">
            {REGIONS.map((reg) => (
              <Checkbox
                key={reg.id}
                checked={draft.regions.excluded.includes(reg.id)}
                onChange={(v) =>
                  patch((d) => void (d.regions.excluded = v ? [...new Set([...d.regions.excluded, reg.id])] : d.regions.excluded.filter((x) => x !== reg.id)))
                }
                label={
                  <span>
                    {reg.label} <span className="text-fg-4">– {reg.hint}</span>
                  </span>
                }
              />
            ))}
          </div>
        </FieldRow>
      </Section>

      <Section title="Admin" description="Who controls the server in-game." icon={<Crown />} bodyClassName="divide-y divide-line">
        <FieldRow
          label="Admin password"
          description="In-game: open the console and type MakeMeAdmin <password>, or use Settings → Server Settings. Admins can change game rules live – the manager keeps those changes."
          error={!ident.adminPassword ? 'Without it nobody can become admin in-game.' : undefined}
        >
          <SecretInput value={ident.adminPassword} invalid={!ident.adminPassword} onChange={(v) => patch((d) => void (d.identity.adminPassword = v))} generate />
        </FieldRow>
      </Section>

      <Section
        title="Network"
        description="Forward game, game + 1 and query (all UDP) on your router for players outside your network – see Port Forwarding."
        icon={<Network />}
        bodyClassName="divide-y divide-line"
      >
        <FieldRow
          label="Game port"
          description={`-Port. Default 7777. The server also uses the next port (${pingerPort(draft)}) – a second server on this PC should use ${l.port + 2}.`}
          error={clash.length ? `Overlaps with ${clash.map((c) => c.name).join(', ')} – they can't run at the same time.` : undefined}
        >
          <div className="flex gap-2">
            <NumberInput value={l.port} min={1024} max={65534} onChange={(v) => patch((d) => void (d.launch.port = v))} className="flex-1" />
            <Button
              size="md"
              icon={<Wand2 className="size-3.5" />}
              onClick={async () => {
                const p = await api.suggestPorts(id);
                patch((d) => {
                  d.launch.port = p.game;
                  d.launch.queryPort = p.query;
                });
              }}
            >
              Free ports
            </Button>
          </div>
        </FieldRow>
        <FieldRow label="Query port" description="-QueryPort. Steam server-browser queries (default 27015). Without it reachable, the server doesn’t appear in the in-game list." error={queryErr}>
          <NumberInput value={l.queryPort} min={1024} max={65535} onChange={(v) => patch((d) => void (d.launch.queryPort = v))} />
        </FieldRow>
        <FieldRow label="Bind to IP" description="-MULTIHOME. Only for PCs with several network adapters. Empty listens on all of them.">
          <Select value={l.multihome} onChange={(e) => patch((d) => void (d.launch.multihome = e.target.value))}>
            <option value="">All interfaces (default)</option>
            {net?.local.map((n) => (
              <option key={n.ip + n.interface} value={n.ip}>
                {n.ip} — {n.interface}
              </option>
            ))}
            {l.multihome && !net?.local.some((n) => n.ip === l.multihome) && <option value={l.multihome}>{l.multihome}</option>}
          </Select>
        </FieldRow>
        <FieldRow label="HTTP bind IP" description="-MULTIHOMEHTTP. Address the server uses to register with Funcom’s server list. Empty = same as “Bind to IP”.">
          <Input mono value={l.multihomeHttp} placeholder={l.multihome || 'automatic'} onChange={(e) => patch((d) => void (d.launch.multihomeHttp = e.target.value.trim()))} />
        </FieldRow>
      </Section>

      <Section
        title="RCON"
        description="Remote console (Game.ini [RconPlugin]). The manager uses it for clean shutdowns, live rule changes, player lists, kicks and bans."
        icon={<TerminalSquare />}
        bodyClassName="divide-y divide-line"
      >
        {running && <Callout tone="warn" className="my-3">Stop the server to change RCON – the running server keeps the credentials it started with.</Callout>}
        <FieldRow label="Enabled" description="Strongly recommended. Without RCON, stops can’t save cleanly and settings can’t change live.">
          <Toggle checked={r.enabled} disabled={running} onChange={(v) => patch((d) => void (d.rcon.enabled = v))} />
        </FieldRow>
        <FieldRow label="Port (TCP)" description="Default 25575. Each server on this PC needs its own." error={rconClash ? `Also used by ${rconClash.name}.` : undefined}>
          <NumberInput value={r.port} min={1024} max={65535} disabled={running} onChange={(v) => patch((d) => void (d.rcon.port = v))} />
        </FieldRow>
        <FieldRow label="Password" description="Generated for you. Only needed if you also use other RCON tools.">
          <SecretInput value={r.password} onChange={(v) => patch((d) => void (d.rcon.password = v))} generate />
        </FieldRow>
        <FieldRow label="Rate limit (karma)" description="RconMaxKarma – how many commands per minute a client may send before the server blocks it for a minute. The manager paces itself well below this.">
          <NumberInput value={r.maxKarma} min={30} max={1000} step={10} disabled={running} onChange={(v) => patch((d) => void (d.rcon.maxKarma = v))} />
        </FieldRow>
        <FieldRow label="Player list refresh" description="How often the manager asks the server who’s online.">
          <NumberInput value={r.pollSecs} min={5} max={120} suffix="s" onChange={(v) => patch((d) => void (d.rcon.pollSecs = v))} />
        </FieldRow>
      </Section>

      <Section title="Manager profile" description="Only used by this app – not sent to the game." icon={<IdCard />} bodyClassName="divide-y divide-line">
        <FieldRow label="Display name" description="Name of this profile in tabs and the sidebar.">
          <Input value={draft.name} onChange={(e) => patch((d) => void (d.name = e.target.value))} />
        </FieldRow>
        <FieldRow label={<span className="flex items-center gap-2"><Palette className="size-3.5 text-fg-3" /> Tag colour</span>} description="Helps tell servers apart at a glance.">
          <div className="flex gap-2">
            {Object.entries(SERVER_COLORS).map(([k, c]) => (
              <button
                key={k}
                title={c.label}
                onClick={() => patch((d) => void (d.color = k))}
                className={cn('size-7 rounded-full border-2 transition-transform hover:scale-110', draft.color === k ? 'border-fg' : 'border-transparent')}
                style={{ background: c.hex }}
              />
            ))}
          </div>
        </FieldRow>
        <FieldRow label={<span className="flex items-center gap-2"><Gamepad2 className="size-3.5 text-fg-3" /> Notes</span>} description="Anything you want to remember about this server." stacked>
          <TextArea value={draft.notes} onChange={(e) => patch((d) => void (d.notes = e.target.value))} placeholder="Discord invite, rules, who has admin…" />
        </FieldRow>
      </Section>
    </div>
  );
}
