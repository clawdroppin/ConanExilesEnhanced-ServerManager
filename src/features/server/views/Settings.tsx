import { useEffect, useState } from 'react';
import { Crown, Gamepad2, Globe2, IdCard, Network, Palette, Wand2 } from 'lucide-react';
import { cn } from '@/lib/cn';
import { api } from '@/lib/ipc';
import { BEACON_OFFSET, beaconPort, netInfo } from '@/lib/net';
import { SERVER_COLORS } from '@/lib/colors';
import type { NetInfo, PlatformPolicy } from '@/lib/types';
import { useApp } from '@/store/app';
import { Button } from '@/components/ui/Button';
import { FieldRow, Input, NumberInput, SecretInput, Select, Slider, TextArea, Toggle } from '@/components/ui/Field';
import { Badge, Callout, PageHeader, Section } from '@/components/ui/Surface';
import { useDraft } from '../draft';

export const PLATFORMS: { value: PlatformPolicy; label: string; hint: string }[] = [
  { value: 'Crossplay', label: 'Crossplay', hint: 'Everyone – PC and consoles' },
  { value: 'PC', label: 'PC only', hint: 'Steam / PC players' },
  { value: 'PlayStation', label: 'PlayStation', hint: 'PlayStation players' },
  { value: 'Xbox', label: 'Xbox', hint: 'Xbox players' },
  { value: 'Nintendo', label: 'Nintendo', hint: 'Nintendo Switch players' },
];

export const OWNER_RE = /^[0-9a-f]{32}$/i;

export function ownerIdProblem(v: string): string | null {
  const t = v.trim();
  if (!t) return null;
  if (!OWNER_RE.test(t)) return 'Player IDs are 32 hexadecimal characters (0-9, A-F). Copy it with the button next to it in-game.';
  return null;
}

function Len({ v, max }: { v: string; max: number }) {
  return <span className={cn('font-mono text-[10.5px]', v.length > max ? 'text-warn' : 'text-fg-4')}>{v.length}/{max}</span>;
}

export function Settings({ id }: { id: string }) {
  const { draft, patch } = useDraft(id);
  const instances = useApp((s) => s.instances);
  const setView = useApp((s) => s.setView);
  const [net, setNet] = useState<NetInfo | null>(null);
  useEffect(() => {
    netInfo().then(setNet).catch(() => {});
  }, []);
  if (!draft) return null;
  const s = draft.ini ?? { ownerId: '', serverName: '', defaultWorldName: '', adminPassword: '', worldPassword: '', platformPolicy: 'Crossplay' as const, allowCrashDumps: true };
  const l = draft.launch;
  const mine = [l.port, beaconPort(draft), l.queryPort].filter((p) => p > 0);
  const clash = instances.filter((o) => o.id !== id && [o.launch.port, beaconPort(o), o.launch.queryPort].some((p) => p > 0 && mine.includes(p)));
  const ownerErr = ownerIdProblem(s.ownerId);

  return (
    <div className="mx-auto max-w-[1000px] space-y-5 p-6 pb-24">
      <PageHeader
        title="Server Settings"
        description="Everything the dedicated server reads from DedicatedServer.ini, plus networking. Saved into the file automatically – changes apply on the next start."
      />

      <Callout tone="info" title="Where are difficulty, PvP and other world rules?">
        They aren’t server config – they’re stored inside the world save. Change them in-game as the owner (Pause → Settings → Server Management), or create a world with the rules you want in single-player and bring it over under{' '}
        <button className="text-accent hover:underline" onClick={() => setView(id, 'worlds')}>
          Worlds
        </button>
        .
      </Callout>

      <Section title="Listing" description="What players see and search for in the in-game server browser." icon={<Globe2 />} bodyClassName="divide-y divide-line">
        <FieldRow
          label="Server name"
          description="Shown as “Created by” in the Public list. Not searchable."
          badge={<Badge tone="err">Required</Badge>}
          hint={<Len v={s.serverName} max={16} />}
          error={!s.serverName.trim() ? 'The server won’t start without a name.' : undefined}
        >
          <Input value={s.serverName} invalid={!s.serverName.trim()} onChange={(e) => patch((d) => void (d.ini.serverName = e.target.value))} />
        </FieldRow>
        <FieldRow
          label="Default world name"
          description="Name of the world the server generates when SaveGames has no world yet – players then search this name. An imported world keeps its own name (its .sav file name); changing this later doesn’t rename an existing world."
          badge={<Badge tone="err">Required</Badge>}
          hint={<Len v={s.defaultWorldName} max={16} />}
          error={!s.defaultWorldName.trim() ? 'Required.' : undefined}
        >
          <Input value={s.defaultWorldName} invalid={!s.defaultWorldName.trim()} onChange={(e) => patch((d) => void (d.ini.defaultWorldName = e.target.value))} />
        </FieldRow>
        <FieldRow label="World password" description="Players need it to join. Leave empty for a public world. Overrides any password stored in the world save.">
          <SecretInput value={s.worldPassword} onChange={(v) => patch((d) => void (d.ini.worldPassword = v))} placeholder="Open to everyone" generate />
        </FieldRow>
        <FieldRow label="Platforms allowed" description="PlatformPolicy – which platforms can join this server.">
          <Select value={s.platformPolicy} onChange={(e) => patch((d) => void (d.ini.platformPolicy = e.target.value))}>
            {PLATFORMS.map((p) => (
              <option key={p.value} value={p.value}>
                {p.label} — {p.hint}
              </option>
            ))}
          </Select>
        </FieldRow>
        <FieldRow
          label="Max players"
          description="Player cap. Funcom supports up to 100 per dedicated server; plan on 2 GB RAM + 1 GB per player."
          hint={`Recommended RAM: ${2 + l.maxPlayers} GB`}
        >
          <div className="flex items-center gap-3">
            <Slider value={l.maxPlayers} min={1} max={100} onChange={(v) => patch((d) => void (d.launch.maxPlayers = v))} />
            <span className="w-8 text-right font-mono text-[13px] text-fg">{l.maxPlayers}</span>
          </div>
        </FieldRow>
      </Section>

      <Section title="Ownership & admin" description="Who controls the server in-game." icon={<Crown />} bodyClassName="divide-y divide-line">
        <FieldRow
          label="Owner ID"
          description={
            <>
              Your in-game <b className="text-fg-2">Player ID</b> – at the bottom of the game’s Settings menu (there’s a copy button). The owner can ban and unban anyone, online or offline.
            </>
          }
          badge={<Badge tone="err">Required</Badge>}
          error={ownerErr ?? (!s.ownerId ? 'The server refuses to host without an Owner ID.' : undefined)}
        >
          <Input mono value={s.ownerId} invalid={!!ownerErr || !s.ownerId} placeholder="32-character Player ID" onChange={(e) => patch((d) => void (d.ini.ownerId = e.target.value.trim()))} />
        </FieldRow>
        <FieldRow
          label="Admin password"
          description="Anyone who enters it under Pause → Settings → Server Management becomes an admin (can ban online players). Changing it revokes admin from everyone."
          badge={<Badge tone="err">Required</Badge>}
          error={!s.adminPassword ? 'The server won’t start without an admin password.' : undefined}
        >
          <SecretInput value={s.adminPassword} invalid={!s.adminPassword} onChange={(v) => patch((d) => void (d.ini.adminPassword = v))} generate />
        </FieldRow>
        <FieldRow label="Send crash reports to Funcom" description="bAllowSendingCrashDumps – lets the server upload crash dumps so Funcom can fix bugs.">
          <Toggle checked={s.allowCrashDumps} onChange={(v) => patch((d) => void (d.ini.allowCrashDumps = v))} />
        </FieldRow>
      </Section>

      <Section
        title="Network"
        description="Both ports are UDP and both must be forwarded on your router for players outside your network – see Port Forwarding."
        icon={<Network />}
        bodyClassName="divide-y divide-line"
      >
        <FieldRow
          label="Game port"
          description="-Port. Funcom’s default is 7777; a second server on the same PC uses 7778, and so on."
          error={clash.length ? `Also used by ${clash.map((c) => c.name).join(', ')} – they can't run at the same time.` : undefined}
        >
          <div className="flex gap-2">
            <NumberInput value={l.port} min={1024} max={65535} onChange={(v) => patch((d) => void (d.launch.port = v))} className="flex-1" />
            <Button
              size="md"
              icon={<Wand2 className="size-3.5" />}
              onClick={async () => {
                const p = await api.suggestPorts(id);
                patch((d) => void (d.launch.port = p.game));
              }}
            >
              Free port
            </Button>
          </div>
        </FieldRow>
        <FieldRow
          label="Beacon port"
          description={`-BeaconPort. Used for server discovery and the join handshake. Funcom’s server image always uses game port + ${BEACON_OFFSET} (7777 → 8888).`}
          hint={l.beaconPort === 0 ? `Automatic: ${beaconPort(draft)}` : 'Custom – remember to forward it too.'}
        >
          <div className="flex items-center gap-3">
            <NumberInput
              value={beaconPort(draft)}
              min={1024}
              max={65535}
              disabled={l.beaconPort === 0}
              onChange={(v) => patch((d) => void (d.launch.beaconPort = v))}
              className="flex-1"
            />
            <label className="flex shrink-0 items-center gap-2 text-[12px] text-fg-3">
              Auto
              <Toggle size="sm" checked={l.beaconPort === 0} onChange={(v) => patch((d) => void (d.launch.beaconPort = v ? 0 : d.launch.port + BEACON_OFFSET))} />
            </label>
          </div>
        </FieldRow>
        <FieldRow label="Query port" description="-QueryPort. Conan Exiles lists servers through Epic Online Services, so this is normally not needed. 0 = not passed.">
          <NumberInput value={l.queryPort} min={0} max={65535} onChange={(v) => patch((d) => void (d.launch.queryPort = v))} />
        </FieldRow>
        <FieldRow label="Bind to IP" description="-MULTIHOME. Only needed on PCs with several network adapters. Empty listens on all of them.">
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
          <TextArea value={draft.notes} onChange={(e) => patch((d) => void (d.notes = e.target.value))} placeholder="Discord invite, mod list, who has admin…" />
        </FieldRow>
      </Section>
    </div>
  );
}
