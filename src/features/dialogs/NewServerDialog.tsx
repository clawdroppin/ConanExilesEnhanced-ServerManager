import { useEffect, useState } from 'react';
import { AnimatePresence, motion } from 'motion/react';
import { ArrowLeft, ArrowRight, Check, Crown, FolderOpen, Globe2, Rocket, Server } from 'lucide-react';
import { cn } from '@/lib/cn';
import { api, errMsg } from '@/lib/ipc';
import { bytes, generatePassword } from '@/lib/format';
import { SERVER_COLORS } from '@/lib/colors';
import { pickFolder } from '@/lib/shell';
import { BEACON_OFFSET, beaconPort } from '@/lib/net';
import type { DedicatedSettings, FolderProbe } from '@/lib/types';
import { useApp } from '@/store/app';
import { Button } from '@/components/ui/Button';
import { Dialog } from '@/components/ui/Dialog';
import { Checkbox, Input, Label, NumberInput, SecretInput, Select, Slider } from '@/components/ui/Field';
import { Callout } from '@/components/ui/Surface';
import { ownerIdProblem, PLATFORMS } from '@/features/server/views/Settings';

const STEPS = [
  { title: 'Server', icon: Server },
  { title: 'World & access', icon: Globe2 },
  { title: 'Launch', icon: Rocket },
];

export function NewServerDialog() {
  const dialog = useApp((s) => s.dialog);
  const setDialog = useApp((s) => s.setDialog);
  const instances = useApp((s) => s.instances);
  const open = dialog?.kind === 'new';
  const [step, setStep] = useState(0);
  const [name, setName] = useState('');
  const [color, setColor] = useState('ember');
  const [dir, setDir] = useState('');
  const [port, setPort] = useState(7777);
  const [maxPlayers, setMaxPlayers] = useState(16);
  const [template, setTemplate] = useState('');
  const [ini, setIni] = useState<DedicatedSettings>({ ownerId: '', serverName: '', defaultWorldName: '', adminPassword: '', worldPassword: '', platformPolicy: 'Crossplay', allowCrashDumps: true });
  const [installNow, setInstallNow] = useState(true);
  const [probe, setProbe] = useState<FolderProbe | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    const n = instances.length + 1;
    setStep(0);
    setName(n === 1 ? 'My Conan Exiles Server' : `Conan Exiles Server ${n}`);
    setColor(Object.keys(SERVER_COLORS)[instances.length % Object.keys(SERVER_COLORS).length]);
    setDir('');
    setTemplate(dialog?.kind === 'new' ? (dialog.templateId ?? '') : '');
    setIni({ ownerId: instances[0]?.ini?.ownerId ?? '', serverName: '', defaultWorldName: '', adminPassword: generatePassword(12), worldPassword: '', platformPolicy: 'Crossplay', allowCrashDumps: true });
    setMaxPlayers(6);
    setInstallNow(true);
    setError(null);
    api.suggestPorts().then((p) => setPort(p.game)).catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  useEffect(() => {
    if (!dir) return setProbe(null);
    const t = setTimeout(() => api.probeFolder(dir).then(setProbe).catch(() => setProbe(null)), 250);
    return () => clearTimeout(t);
  }, [dir]);

  const serverName = ini.serverName || name.slice(0, 16);
  const worldName = ini.defaultWorldName || name.replace(/[^A-Za-z0-9]/g, '').slice(0, 16) || 'MyWorld';
  const ownerErr = ownerIdProblem(ini.ownerId);
  const portClash = instances.find((i) => [i.launch.port, beaconPort(i), i.launch.queryPort].some((p) => p > 0 && (p === port || p === port + BEACON_OFFSET)));
  const canNext = step === 0 ? name.trim().length > 0 && !portClash : step === 1 ? !!ini.adminPassword && !ownerErr : true;

  const create = async () => {
    setBusy(true);
    setError(null);
    try {
      const inst = await api.createInstance({
        name: name.trim(),
        installDir: dir || null,
        color,
        identity: { serverName, serverPassword: ini.worldPassword, adminPassword: ini.adminPassword, motd: '' },
        ini: { ...ini, serverName, defaultWorldName: worldName },
        port,
        maxPlayers,
        installNow,
        templateId: template || null,
      });
      useApp.getState().upsertInstance(inst);
      setDialog(null);
      useApp.getState().openServer(inst.id, 'overview');
      useApp.getState().toast('success', installNow ? `${inst.name} created – downloading server files…` : `${inst.name} created`, inst.id);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={open}
      onClose={() => setDialog(null)}
      title="Create a Conan Exiles server"
      description="Three quick steps. Everything can be changed later."
      icon={<Rocket />}
      width={640}
      footer={
        <>
          {step > 0 && (
            <Button variant="ghost" icon={<ArrowLeft className="size-3.5" />} onClick={() => setStep(step - 1)}>
              Back
            </Button>
          )}
          <div className="flex-1" />
          {step < 2 ? (
            <Button variant="primary" iconRight={<ArrowRight className="size-3.5" />} disabled={!canNext} onClick={() => setStep(step + 1)}>
              Continue
            </Button>
          ) : (
            <Button variant="primary" icon={<Check className="size-4" />} loading={busy} onClick={create}>
              {installNow ? 'Create & install' : 'Create server'}
            </Button>
          )}
        </>
      }
    >
      <div className="mb-5 flex items-center gap-2">
        {STEPS.map((s, i) => (
          <div key={s.title} className="flex flex-1 items-center gap-2">
            <button
              disabled={i > step}
              onClick={() => setStep(i)}
              className={cn(
                'flex items-center gap-2 rounded-lg px-2 py-1 text-[12px] font-medium transition-colors',
                i === step ? 'text-fg' : i < step ? 'text-ok' : 'text-fg-4',
              )}
            >
              <span className={cn('flex size-6 items-center justify-center rounded-full border text-[11px]', i === step ? 'border-accent bg-accent/15 text-accent' : i < step ? 'border-ok/40 bg-ok/10' : 'border-line-strong')}>
                {i < step ? <Check className="size-3" /> : i + 1}
              </span>
              {s.title}
            </button>
            {i < STEPS.length - 1 && <div className={cn('h-px flex-1', i < step ? 'bg-ok/40' : 'bg-line')} />}
          </div>
        ))}
      </div>

      <AnimatePresence mode="wait">
        <motion.div key={step} initial={{ opacity: 0, x: 8 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -8 }} transition={{ duration: 0.13 }} className="space-y-4">
          {step === 0 && (
            <>
              <div>
                <Label>Profile name</Label>
                <Input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="My Conan Exiles Server" />
              </div>
              <div>
                <Label>Colour</Label>
                <div className="flex gap-2">
                  {Object.entries(SERVER_COLORS).map(([k, c]) => (
                    <button key={k} title={c.label} onClick={() => setColor(k)} className={cn('size-7 rounded-full border-2 transition-transform hover:scale-110', color === k ? 'border-fg' : 'border-transparent')} style={{ background: c.hex }} />
                  ))}
                </div>
              </div>
              <div>
                <Label>Install folder</Label>
                <div className="flex gap-2">
                  <Input mono value={dir} onChange={(e) => setDir(e.target.value)} placeholder="Automatic (inside the manager’s data folder)" />
                  <Button
                    icon={<FolderOpen className="size-3.5" />}
                    onClick={async () => {
                      const p = await pickFolder('Choose where to install the server');
                      if (p) setDir(p);
                    }}
                  >
                    Browse
                  </Button>
                </div>
                {probe && (
                  <div className={cn('mt-1.5 text-[11.5px]', !probe.writable ? 'text-err' : probe.hasServer ? 'text-ok' : 'text-fg-4')}>
                    {!probe.writable
                      ? 'This folder is not writable.'
                      : probe.hasServer
                        ? 'A Conan Exiles server is already installed here – it will be used as-is.'
                        : `${probe.empty ? 'Empty folder' : 'Folder has files'}${probe.freeBytes != null ? ` · ${bytes(probe.freeBytes)} free` : ''}`}
                  </div>
                )}
              </div>
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <Label>Game port (UDP)</Label>
                  <NumberInput value={port} min={1024} max={65535} onChange={setPort} />
                  {portClash ? (
                    <div className="mt-1.5 text-[11.5px] text-err">Clashes with {portClash.name}.</div>
                  ) : (
                    <div className="mt-1.5 text-[11.5px] text-fg-4">Beacon UDP {port + BEACON_OFFSET} is set automatically (Funcom’s default pair).</div>
                  )}
                </div>
                {instances.length > 0 && (
                  <div>
                    <Label>Copy settings from</Label>
                    <Select value={template} onChange={(e) => setTemplate(e.target.value)}>
                      <option value="">Defaults</option>
                      {instances.map((i) => (
                        <option key={i.id} value={i.id}>
                          {i.name}
                        </option>
                      ))}
                    </Select>
                  </div>
                )}
              </div>
            </>
          )}
          {step === 1 && (
            <>
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <Label>Server name</Label>
                  <Input value={ini.serverName} placeholder={name.slice(0, 16)} onChange={(e) => setIni({ ...ini, serverName: e.target.value })} />
                </div>
                <div>
                  <Label>World name (players search this)</Label>
                  <Input value={ini.defaultWorldName} placeholder={worldName} onChange={(e) => setIni({ ...ini, defaultWorldName: e.target.value })} />
                </div>
              </div>
              <div>
                <Label>
                  <span className="flex items-center gap-1.5">
                    <Crown className="size-3 text-gold" /> Your Player ID (Owner ID)
                  </span>
                </Label>
                <Input mono value={ini.ownerId} invalid={!!ownerErr} onChange={(e) => setIni({ ...ini, ownerId: e.target.value.trim() })} placeholder="Bottom of the in-game Settings menu – click copy" />
                {ownerErr ? <div className="mt-1.5 text-[11.5px] text-err">{ownerErr}</div> : <div className="mt-1.5 text-[11.5px] text-fg-4">Required before the first start – it makes you the owner (ban and unban anyone). You can add it later under Server Settings.</div>}
              </div>
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <Label>Admin password</Label>
                  <SecretInput value={ini.adminPassword} onChange={(v) => setIni({ ...ini, adminPassword: v })} generate />
                </div>
                <div>
                  <Label>World password (optional)</Label>
                  <SecretInput value={ini.worldPassword} onChange={(v) => setIni({ ...ini, worldPassword: v })} placeholder="Open to everyone" />
                </div>
              </div>
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <Label>Platforms</Label>
                  <Select value={ini.platformPolicy} onChange={(e) => setIni({ ...ini, platformPolicy: e.target.value })}>
                    {PLATFORMS.map((p) => (
                      <option key={p.value} value={p.value}>
                        {p.label}
                      </option>
                    ))}
                  </Select>
                </div>
                <div>
                  <Label>Max players · {maxPlayers}</Label>
                  <Slider value={maxPlayers} min={1} max={100} onChange={setMaxPlayers} marks={[1, 4, 8, 16, 32, 64, 100]} />
                </div>
              </div>
            </>
          )}
          {step === 2 && (
            <>
              <div className="rounded-xl border border-line bg-bg-raised p-4">
                <dl className="grid grid-cols-[140px_1fr] gap-y-2 text-[12.5px]">
                  <dt className="text-fg-3">Profile</dt>
                  <dd className="text-fg">{name}</dd>
                  <dt className="text-fg-3">Server / world</dt>
                  <dd className="text-fg">
                    {serverName} · <span className="font-mono">{worldName}</span>
                  </dd>
                  <dt className="text-fg-3">Port</dt>
                  <dd className="font-mono text-fg">
                    UDP {port} (game) + {port + BEACON_OFFSET} (beacon)
                  </dd>
                  <dt className="text-fg-3">Players</dt>
                  <dd className="text-fg">
                    up to {maxPlayers} · {PLATFORMS.find((p) => p.value === ini.platformPolicy)?.label}
                  </dd>
                  <dt className="text-fg-3">Access</dt>
                  <dd className="text-fg">{ini.worldPassword ? 'Password protected' : 'Open'}</dd>
                  <dt className="text-fg-3">Folder</dt>
                  <dd className="truncate font-mono text-[11.5px] text-fg">{dir || 'automatic'}</dd>
                </dl>
              </div>
              <Checkbox checked={installNow} onChange={setInstallNow} label="Download the server files now with SteamCMD (several GB, no Steam login needed)" />
              <Callout tone="info">
                After the download, press <b className="text-fg">Start</b>. The first start creates a fresh world named “{worldName}”. To host a world you already play locally, import it under <b className="text-fg">Worlds</b>.
              </Callout>
              {error && <Callout tone="err">{error}</Callout>}
            </>
          )}
        </motion.div>
      </AnimatePresence>
    </Dialog>
  );
}
