import { useEffect, useState } from 'react';
import { api } from './ipc';
import { MAPS, mergeMaps, type GameMap } from './maps';

/**
 * The maps a server can run: the built-in list (with friendly labels/hints) plus any maps
 * discovered in the installed game. New official maps and mod maps appear automatically.
 *
 * Falls back to the built-in list when there's no instance yet (create wizard) or the scan
 * fails, so the dropdown is never empty.
 */
export function useMaps(id: string | null): GameMap[] {
  const [maps, setMaps] = useState<GameMap[]>(MAPS);
  useEffect(() => {
    if (!id) return;
    let live = true;
    api
      .listMaps(id)
      .then((fetched) => {
        if (live) setMaps(mergeMaps(fetched));
      })
      .catch(() => {
        if (live) setMaps(MAPS);
      });
    return () => {
      live = false;
    };
  }, [id]);
  return maps;
}
