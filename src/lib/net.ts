// Shared network helpers: Funcom's port scheme and a short-lived cache for slow lookups.
import { api } from './ipc';
import type { NetInfo, ServerInstance } from './types';

/** Funcom's server image derives the beacon port as game port + 1111 (7777 → 8888). */
export const BEACON_OFFSET = 1111;

export function beaconPort(inst: Pick<ServerInstance, 'launch'>): number {
  return inst.launch.beaconPort > 0 ? inst.launch.beaconPort : inst.launch.port + BEACON_OFFSET;
}

export interface PortRow {
  port: number;
  label: string;
  purpose: string;
  required: boolean;
}

/** Every UDP port the server binds – what to forward and allow through the firewall. */
export function serverPorts(inst: ServerInstance): PortRow[] {
  const rows: PortRow[] = [
    { port: inst.launch.port, label: 'Game', purpose: 'Player connections and gameplay traffic', required: true },
    { port: beaconPort(inst), label: 'Beacon', purpose: 'World-settings beacon – the in-game browser reads world info and join details through it (game port + 1111)', required: true },
  ];
  if (inst.launch.queryPort > 0) rows.push({ port: inst.launch.queryPort, label: 'Query', purpose: 'Optional Steam query port', required: false });
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
