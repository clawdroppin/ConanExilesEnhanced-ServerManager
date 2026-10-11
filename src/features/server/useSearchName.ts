import { useEffect, useState } from 'react';
import { api } from '@/lib/ipc';
import { useInstance, useStatus } from '@/store/app';

/**
 * The name players search for in Worlds → Public. That's the loaded world's save slot (the .sav
 * file name), which equals Default World Name only for a world the server generated itself –
 * an imported world keeps its own name.
 */
export function useSearchName(id: string): string {
  const inst = useInstance(id);
  const st = useStatus(id);
  const [active, setActive] = useState<string | null>(null);
  const live = st?.state === 'running' || st?.state === 'starting';
  useEffect(() => {
    api
      .listSaves(id)
      .then((s) => setActive(s.find((x) => x.active)?.name.replace(/\.sav$/i, '') ?? null))
      .catch(() => setActive(null));
  }, [id, st?.state]);
  if (live && st?.session?.SlotName) return st.session.SlotName;
  return active ?? inst?.ini?.defaultWorldName ?? '';
}
