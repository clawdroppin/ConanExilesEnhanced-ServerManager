// Shared network helpers: Funcom's port scheme and a short-lived cache for slow lookups.
import { api } from './ipc';
import type { NetInfo, ServerInstance } from './types';

/** Conan Exiles also binds game port + 1 (UDP) for its pinger/raw-UDP socket (7777 → 7778). */
export function pingerPort(inst: Pick<ServerInstance, 'launch'>): number {
  return inst.launch.port + 1;
}

export interface PortRow {
  port: number;
  label: string;
  purpose: string;
  proto: 'UDP' | 'TCP';
  required: boolean;
}

/** Every port to forward / allow through the firewall (Funcom's list for Conan Exiles). */
export function serverPorts(inst: ServerInstance): PortRow[] {
  const rows: PortRow[] = [
    { port: inst.launch.port, label: 'Game', purpose: 'Player connections and gameplay traffic', proto: 'UDP', required: true },
    { port: pingerPort(inst), label: 'Pinger', purpose: 'Raw UDP socket the server always opens on game port + 1', proto: 'UDP', required: true },
    { port: inst.launch.queryPort, label: 'Query', purpose: 'Steam server-browser queries – without it the server doesn’t show up in the list', proto: 'UDP', required: true },
  ];
  if (inst.rcon.enabled) rows.push({ port: inst.rcon.port, label: 'RCON', proto: 'TCP', purpose: 'Remote admin. Only forward it if you use RCON tools from outside your network – the manager doesn’t need it.', required: false });
  return rows;
}

let netCache: { at: number; value: Promise<NetInfo> } | null = null;

/** Public IP + LAN addresses + gateway, cached for 5 minutes (the public IP lookup is an HTTP call). */
export function netInfo(force = false): Promise<NetInfo> {
  if (!force && netCache && Date.now() - netCache.at < 300_000) return netCache.value;
  const value = api.netInfo().catch((e) => {
    netCache = null;
    throw e;
  });
  netCache = { at: Date.now(), value };
  return value;
}

/** The LAN address players' traffic should be forwarded to. */
export function lanAddress(inst: ServerInstance, net: NetInfo | null): string | null {
  if (inst.launch.multihome) return inst.launch.multihome;
  return net?.local[0]?.ip ?? null;
}
