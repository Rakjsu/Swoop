import { useEffect, useRef, useState } from 'react';
import { errorMessage, type Transport } from './index';

/** Espera antes de recarregar: junta rajadas de avisos numa leitura só. */
const REFRESH_DEBOUNCE_MS = 150;

export interface LiveQuery<T> {
  data: T;
  loaded: boolean;
  error: string | null;
}

/**
 * Consulta que se recarrega sozinha quando o backend avisa que a fila mudou.
 * Assina antes da primeira leitura (nenhum aviso se perde no meio) e
 * descarta respostas que chegam fora de ordem.
 */
export function useLiveQuery<T>(
  transport: Transport,
  fetch: (t: Transport) => Promise<T>,
  initial: T,
): LiveQuery<T> {
  const [data, setData] = useState<T>(initial);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fetchRef = useRef(fetch);

  useEffect(() => {
    fetchRef.current = fetch;
  });

  useEffect(() => {
    let alive = true;
    let asked = 0;
    let applied = 0;
    let timer: number | undefined;

    const refresh = () => {
      const mine = ++asked;
      fetchRef
        .current(transport)
        .then((value) => {
          if (!alive || mine < applied) return;
          applied = mine;
          setData(value);
          setLoaded(true);
          setError(null);
        })
        .catch((err: unknown) => {
          if (alive) setError(errorMessage(err));
        });
    };

    const off = transport.onPush((push) => {
      if (push.type === 'snapshot') return;
      window.clearTimeout(timer);
      timer = window.setTimeout(refresh, REFRESH_DEBOUNCE_MS);
    });
    refresh();
    return () => {
      alive = false;
      off();
      window.clearTimeout(timer);
    };
  }, [transport]);

  return { data, loaded, error };
}
