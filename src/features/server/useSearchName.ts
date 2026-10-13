import { useInstance, useStatus } from '@/store/app';

/**
 * The name players see in the server browser (Worlds → Public). For Conan Exiles that is the
 * server's ServerName (Engine.ini [OnlineSubsystem] ServerName). While the server is running we
 * prefer the slot name it reports over the network, since that is exactly what the browser shows.
 */
export function useSearchName(id: string): string {
  const inst = useInstance(id);
  const st = useStatus(id);
  const live = st?.state === 'running' || st?.state === 'starting';
  if (live && st?.session?.SlotName) return st.session.SlotName;
  return inst?.identity?.serverName || '';
}
