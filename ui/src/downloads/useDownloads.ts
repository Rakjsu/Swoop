import { useEffect, useMemo, useState } from 'react';
import type { DownloadView } from '../gen/DownloadView';
import type { LiveRow } from '../gen/LiveRow';
import type { Snapshot } from '../gen/Snapshot';
import type { Transport } from '../transport';
import { useLiveQuery } from '../transport/useLiveQuery';

export interface DownloadsState {
  rows: DownloadView[];
  /** Progresso ao vivo por id (só os ativos). */
  live: Map<number, LiveRow>;
  snapshot: Snapshot | null;
  loaded: boolean;
  error: string | null;
}

const NO_ROWS: DownloadView[] = [];
const listDownloads = (t: Transport) => t.list();

/** Lista do banco (recarregada quando a fila muda) + retrato ao vivo por cima. */
export function useDownloads(transport: Transport): DownloadsState {
  const { data: rows, loaded, error } = useLiveQuery(transport, listDownloads, NO_ROWS);
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);

  useEffect(
    () =>
      transport.onPush((push) => {
        if (push.type === 'snapshot') setSnapshot(push);
      }),
    [transport],
  );

  const live = useMemo(
    () => new Map((snapshot?.active ?? []).map((row) => [row.id, row])),
    [snapshot],
  );
  return { rows, live, snapshot, loaded, error };
}
