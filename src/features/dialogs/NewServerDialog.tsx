import { useEffect, useState } from 'react';
import { AnimatePresence, motion } from 'motion/react';
import { ArrowLeft, ArrowRight, Check, FolderOpen, Globe2, Rocket, Server } from 'lucide-react';
import { cn } from '@/lib/cn';
import { api, errMsg } from '@/lib/ipc';
import { bytes, generatePassword } from '@/lib/format';
import { SERVER_COLORS } from '@/lib/colors';
import { pickFolder } from '@/lib/shell';
import { pingerPort } from '@/lib/net';
import { DEFAULT_MAP, mergeMaps } from '@/lib/maps';
import type { FolderProbe, MapInfo } from '@/lib/types';
import { useApp } from '@/store/app';
import { Button } from '@/components/ui/Button';
import { Dialog } from '@/components/ui/Dialog';
import { Checkbox, Input, Label, NumberInput, SecretInput, Segmented, Select, Slider } from '@/components/ui/Field';
import { Callout } from '@/components/ui/Surface';

/** Starting rules written into ServerSettings.ini before the first start (all editable later under
 *  Game Rules). Values are in file form – DropEquipmentOnDeath: 0 nothing, 1 everything, 2 backpack. */
const DEATH = [
  { value: '0', label: 'Keep everything' },
  { value: '2', label: 'Drop backpack' },
  { value: '1', label: 'Drop everything' },
] as const;

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
  const [serverName, setServerName] = useState('');
  const [map, setMap] = useState(DEFAULT_MAP);
  const [adminPassword, setAdminPassword] = useState('');
  const [joinPassword, setJoinPassword] = useState('');
  const [pvp, setPvp] = useState(false);
  const [death, setDeath] = useState<string>('1');
  const [installNow, setInstallNow] = useState(true);
  const [probe, setProbe] = useState<FolderProbe | null>(null);
  const [discovered, setDiscovered] = useState<MapInfo[]>([]);
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
    setServerName('');
    setMap(DEFAULT_MAP);
    setAdminPassword(generatePassword(12));
    setJoinPassword('');
    setPvp(false);
    setDeath('1');
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

  // Auto-discover maps in the chosen install (new official maps + mod maps appear automatically).
  useEffect(() => {
    if (!dir) return setDiscovered([]);
    let live = true;
    const t = setTimeout(() => api.listMapsIn(dir).then((m) => live && setDiscovered(m)).catch(() => live && setDiscovered([])), 250);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [dir]);

  const mapOptions = mergeMaps(discovered);
  const listedName = serverName.trim() || name.trim();
  const portClash = instances.find((i) => [i.launch.port, pingerPort(i), i.launch.queryPort].some((p) => p > 0 && (p === port || p === port + 1)));
  const canNext = step === 0 ? name.trim().length > 0 && !portClash : step === 1 ? !!adminPassword : true;

  const create = async () => {
    setBusy(true);
    setError(null);
    try {
      const inst = await api.createInstance({
        name: name.trim(),
        installDir: dir || null,
        color,
        identity: { serverName: listedName, serverPassword: joinPassword, adminPassword, motd: '' },
        ini: { defaultMap: map },
        // Copying another server keeps its rules; otherwise seed the two choices made here.
        rules: template ? {} : { PVPEnabled: pvp ? 'True' : 'False', DropEquipmentOnDeath: death },
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
                    <div className="mt-1.5 text-[11.5px] text-fg-4">The server also uses UDP {port + 1}; a free query port is picked automatically.</div>
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
              <div>
                <Label>Server name (shown in the server browser)</Label>
                <Input value={serverName} placeholder={name} onChange={(e) => setServerName(e.target.value)} />
              </div>
              <div>
                <Label>Map</Label>
                <Select value={map} onChange={(e) => setMap(e.target.value)}>
                  {mapOptions.map((m) => (
                    <option key={m.value} value={m.value}>
                      {m.label} — {m.hint}
                    </option>
                  ))}
                </Select>
                <div className="mt-1.5 text-[11.5px] text-fg-4">Each map keeps its own world, so switching maps later starts a fresh world on that map.</div>
              </div>
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <Label>Admin password</Label>
                  <SecretInput value={adminPassword} onChange={setAdminPassword} generate />
                  <div className="mt-1.5 text-[11.5px] text-fg-4">In-game: MakeMeAdmin &lt;password&gt; in the console.</div>
                </div>
                <div>
                  <Label>Join password (optional)</Label>
                  <SecretInput value={joinPassword} onChange={setJoinPassword} placeholder="Open to everyone" />
                </div>
              </div>
              {!template && (
                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <Label>On death, players…</Label>
                    <Segmented value={death} onChange={setDeath} options={DEATH.map((d) => ({ value: d.value, label: d.label }))} />
                  </div>
                  <div>
                    <Label>Mode</Label>
                    <Segmented value={pvp ? 'pvp' : 'pve'} onChange={(v) => setPvp(v === 'pvp')} options={[{ value: 'pve', label: 'PvE' }, { value: 'pvp', label: 'PvP' }]} />
                  </div>
                </div>
              )}
              <div>
                <Label>Max players</Label>
                <div className="flex items-center gap-3">
                  <Slider value={maxPlayers} min={1} max={100} onChange={setMaxPlayers} marks={[1, 4, 8, 16, 32, 64, 100]} />
                  <input
                    type="number"
                    min={1}
                    max={200}
                    value={maxPlayers}
                    onChange={(e) => setMaxPlayers(Math.max(1, Math.min(200, Number(e.target.value) || 1)))}
                    className="h-8 w-16 rounded-md border border-line bg-bg px-2 text-center font-mono text-[13px] text-fg focus:border-accent focus:outline-none"
                  />
                </div>
              </div>
              <div className="text-[11.5px] text-fg-4">All other rules (rates, decay, raids, storms…) are under Game Rules after creating the server.</div>
            </>
          )}
          {step === 2 && (
            <>
              <div className="rounded-xl border border-line bg-bg-raised p-4">
                <dl className="grid grid-cols-[140px_1fr] gap-y-2 text-[12.5px]">
                  <dt className="text-fg-3">Profile</dt>
                  <dd className="text-fg">{name}</dd>
                  <dt className="text-fg-3">Server name</dt>
                  <dd className="text-fg">{listedName}</dd>
                  <dt className="text-fg-3">Ports</dt>
                  <dd className="font-mono text-fg">
                    UDP {port} + {port + 1} (game) · query picked automatically
                  </dd>
                  <dt className="text-fg-3">Players</dt>
                  <dd className="text-fg">
                    up to {maxPlayers}
                    {!template && ` · ${pvp ? 'PvP' : 'PvE'} · ${DEATH.find((d) => d.value === death)?.label.toLowerCase()} on death`}
                  </dd>
                  <dt className="text-fg-3">Access</dt>
                  <dd className="text-fg">{joinPassword ? 'Password protected' : 'Open'}</dd>
                  <dt className="text-fg-3">Folder</dt>
                  <dd className="truncate font-mono text-[11.5px] text-fg">{dir || 'automatic'}</dd>
                </dl>
              </div>
              <Checkbox checked={installNow} onChange={setInstallNow} label="Download the server files now with SteamCMD (several GB, no Steam login needed)" />
              <Callout tone="info">
                After the download, press <b className="text-fg">Start</b>. The first start creates a fresh world. To host a world from another server or single-player, import its game_0.db under <b className="text-fg">Worlds</b>.
              </Callout>
              {error && <Callout tone="err">{error}</Callout>}
            </>
          )}
        </motion.div>
      </AnimatePresence>
    </Dialog>
  );
}
