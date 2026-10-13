import { useCallback, useEffect, useState } from 'react';
import { AlertTriangle, CheckCircle2, Globe2, Info, Loader2, RefreshCw, Router, ShieldCheck, ShieldPlus, XCircle } from 'lucide-react';
import { cn } from '@/lib/cn';
import { api, errMsg } from '@/lib/ipc';
import { copy } from '@/lib/shell';
import { netInfo, pingerPort } from '@/lib/net';
import type { DiagCheck, FirewallState, NetInfo } from '@/lib/types';
import { useApp, useInstance, type ViewId } from '@/store/app';
import { useServerActions } from '@/hooks/useServerActions';
import { Button } from '@/components/ui/Button';
import { Badge, Card, PageHeader, Section } from '@/components/ui/Surface';

const ICON = { ok: CheckCircle2, warn: AlertTriangle, error: XCircle, info: Info };
const COLOR = { ok: 'text-ok', warn: 'text-warn', error: 'text-err', info: 'text-info' };

export function Health({ id }: { id: string }) {
  const inst = useInstance(id)!;
  const toast = useApp((s) => s.toast);
  const setView = useApp((s) => s.setView);
  const { run } = useServerActions(id);
  const [checks, setChecks] = useState<DiagCheck[] | null>(null);
  const [fw, setFw] = useState<FirewallState | null>(null);
  const [net, setNet] = useState<NetInfo | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [running, setRunning] = useState(false);

  const load = useCallback(async () => {
    setRunning(true);
    try {
      const [c, f] = await Promise.all([api.diagnostics(id), api.firewallState(id)]);
      setChecks(c.checks);
      setFw(f);
    } catch (e) {
      toast('error', errMsg(e));
    } finally {
      setRunning(false);
    }
    netInfo().then(setNet).catch(() => {});
  }, [id, toast]);
  useEffect(() => {
    load();
  }, [load]);

  const doFix = async (fix: string) => {
    const go = (v: ViewId) => setView(id, v);
    if (fix === 'settings' || fix === 'ports') return go('settings');
    if (fix === 'log') return go('console');
    if (fix === 'install') return run('install');
    if (fix === 'validate') return run('validate');
    if (fix === 'update') return run('update');
    setBusy(fix);
    try {
      if (fix === 'firewall') {
        await api.addFirewallRules(id);
        toast('success', 'Firewall rules added', id);
      } else if (fix === 'prereqs') {
        await api.installPrereqs(id);
        toast('success', 'Visual C++ runtime installed', id);
      }
      await load();
    } catch (e) {
      toast('error', errMsg(e), id);
    } finally {
      setBusy(null);
    }
  };

  const FIX_LABEL: Record<string, string> = {
    firewall: 'Add rule',
    install: 'Install',
    validate: 'Validate',
    settings: 'Open settings',
    ports: 'Change port',
    prereqs: 'Install runtime',
    update: 'Update now',
    log: 'Open console',
  };
  const problems = (checks ?? []).filter((c) => c.level === 'error' || c.level === 'warn').length;

  return (
    <div className="mx-auto max-w-[1000px] space-y-5 p-6">
      <PageHeader
        title="Network & Health"
        description="Checks everything that commonly stops a Conan Exiles server from starting or being joinable."
        actions={
          <Button size="sm" icon={<RefreshCw className={cn('size-3.5', running && 'animate-spin')} />} onClick={load} disabled={running}>
            Run checks
          </Button>
        }
      />
      <Section
        title={
          <span className="flex items-center gap-2">
            Health checks {checks && <Badge tone={problems ? 'warn' : 'ok'}>{problems ? `${problems} to look at` : 'All good'}</Badge>}
          </span>
        }
        icon={<ShieldCheck />}
        bodyClassName="divide-y divide-line"
      >
        {checks === null ? (
          <div className="flex items-center justify-center gap-2 py-10 text-[12.5px] text-fg-3">
            <Loader2 className="size-4 animate-spin" /> Checking…
          </div>
        ) : (
          checks.map((c) => {
            const Icon = ICON[c.level as keyof typeof ICON] ?? Info;
            return (
              <div key={c.id + c.title} className="flex items-start gap-3 py-3">
                <Icon className={cn('mt-0.5 size-4 shrink-0', COLOR[c.level as keyof typeof COLOR])} />
                <div className="min-w-0 flex-1">
                  <div className="text-[12.5px] font-medium text-fg">{c.title}</div>
                  <div className="selectable mt-0.5 text-[12px] leading-relaxed whitespace-pre-wrap text-fg-3">{c.detail}</div>
                </div>
                {c.fix && c.level !== 'ok' && (
                  <Button size="sm" variant={c.level === 'error' ? 'primary' : 'secondary'} loading={busy === c.fix} onClick={() => doFix(c.fix)}>
                    {FIX_LABEL[c.fix] ?? 'Fix'}
                  </Button>
                )}
              </div>
            );
          })
        )}
      </Section>

      <div className="grid grid-cols-2 gap-4">
        <Section title="Windows Firewall" icon={<ShieldPlus />} bodyClassName="py-4 space-y-3">
          <div className="flex items-center justify-between text-[12.5px]">
            <span className="text-fg-3">Inbound rule</span>
            {fw == null ? (
              <span className="text-fg-4">…</span>
            ) : fw.blockedBy.length ? (
              <Badge tone="err">Blocked</Badge>
            ) : fw.portsOk || fw.programRule ? (
              <Badge tone="ok">Allowed</Badge>
            ) : (
              <Badge tone="warn">Missing</Badge>
            )}
          </div>
          {fw?.rulePorts && (
            <div className="flex items-center justify-between text-[12.5px]">
              <span className="text-fg-3">UDP ports in rule</span>
              <span className="font-mono text-fg">{fw.rulePorts}</span>
            </div>
          )}
          <div className="flex gap-2 pt-1">
            <Button size="sm" variant="primary" icon={<ShieldPlus className="size-3.5" />} loading={busy === 'firewall'} onClick={() => doFix('firewall')}>
              {fw?.blockedBy.length ? 'Unblock & allow' : fw?.ruleExists ? 'Recreate rules' : 'Allow through firewall'}
            </Button>
            {fw?.ruleExists && (
              <Button
                size="sm"
                variant="ghost"
                onClick={async () => {
                  try {
                    await api.removeFirewallRules(id);
                    load();
                  } catch (e) {
                    toast('error', errMsg(e));
                  }
                }}
              >
                Remove
              </Button>
            )}
          </div>
          {fw && fw.blockedBy.length > 0 && <div className="text-[11.5px] text-err">Block rule “{fw.blockedBy[0]}” stops players from connecting – the button below removes it.</div>}
          <div className="text-[11.5px] text-fg-4">Adds inbound UDP {inst.launch.port} + {pingerPort(inst)} + {inst.launch.queryPort} and a program rule. Windows asks for administrator permission once.</div>
        </Section>
        <Section title="Addresses" icon={<Globe2 />} bodyClassName="py-4 space-y-2">
          <div className="flex items-center justify-between text-[12.5px]">
            <span className="text-fg-3">Public IP</span>
            <button className="selectable font-mono text-fg hover:text-accent" onClick={() => net?.publicIp && copy(net.publicIp)}>
              {net?.publicIp ?? '…'}
            </button>
          </div>
          {net?.local.map((n) => (
            <div key={n.ip + n.interface} className="flex items-center justify-between text-[12.5px]">
              <span className="truncate pr-3 text-fg-3">{n.interface}</span>
              <button className="selectable font-mono text-fg hover:text-accent" onClick={() => copy(n.ip)}>
                {n.ip}
              </button>
            </div>
          ))}
        </Section>
      </div>

      <Card className="flex items-center gap-4 p-5">
        <Router className="size-5 shrink-0 text-accent" />
        <div className="min-w-0 flex-1">
          <div className="text-[13px] font-semibold text-fg">Port forwarding</div>
          <div className="mt-0.5 text-[12.5px] text-fg-3">
            Forward UDP <span className="font-mono text-fg">{inst.launch.port}</span>, <span className="font-mono text-fg">{pingerPort(inst)}</span> and <span className="font-mono text-fg">{inst.launch.queryPort}</span> to this PC – automatically over UPnP or step by step.
          </div>
        </div>
        <Button size="sm" variant="primary" onClick={() => setView(id, 'ports')}>
          Open guide
        </Button>
      </Card>
    </div>
  );
}
