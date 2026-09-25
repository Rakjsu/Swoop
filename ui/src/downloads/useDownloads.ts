import { useEffect, useMemo, useState } from 'react';
import type { DownloadView } from '../gen/DownloadView';
import type { LiveRow } from '../gen/LiveRow';
import type { Snapshot } from '../gen/Snapshot';
import { errorMessage, type Transport } from '../transport';

/** Espera antes de recarregar a lista: junta rajadas de avisos numa leitura. */
const REFRESH_DEBOUNCE_MS = 150;

export interface DownloadsState {
  rows: DownloadView[];
  /** Progresso ao vivo por id (só os ativos). */
  live: Map<number, LiveRow>;
  snapshot: Snapshot | null;
  loaded: boolean;
  error: string | null;
}

/**
 * Lista do banco + retrato ao vivo por cima. Assina antes de listar (nenhum
 * aviso se perde entre as duas coisas) e descarta respostas de `list` que
 * chegarem fora de ordem.
 */
export function useDownloads(transport: Transport): DownloadsState {
  const [rows, setRows] = useState<DownloadView[]>([]);
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    let asked = 0;
    let applied = 0;
    let timer: number | undefined;

    const refresh = () => {
      const mine = ++asked;
      transport
        .list()
        .then((list) => {
          if (!alive || mine < applied) return;
          applied = mine;
          setRows(list);
          setLoaded(true);
          setError(null);
        })
        .catch((err: unknown) => {
          if (alive) setError(errorMessage(err));
        });
    };

    const off = transport.onPush((push) => {
      if (push.type === 'snapshot') {
        setSnapshot(push);
      } else {
        window.clearTimeout(timer);
        timer = window.setTimeout(refresh, REFRESH_DEBOUNCE_MS);
      }
    });
    refresh();
    return () => {
      alive = false;
      off();
      window.clearTimeout(timer);
    };
  }, [transport]);

  const live = useMemo(
    () => new Map((snapshot?.active ?? []).map((row) => [row.id, row])),
    [snapshot],
  );
  return { rows, live, snapshot, loaded, error };
}
