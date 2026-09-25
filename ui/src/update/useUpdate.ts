import { useCallback, useEffect, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

/** Espelho de `update::UpdateView` (Rust). */
export interface UpdateView {
  version: string;
  notes: string;
}

interface Progress {
  received: number;
  total: number | null;
}

export type UpdateState =
  | { kind: 'idle' }
  | { kind: 'available'; update: UpdateView }
  | { kind: 'downloading'; update: UpdateView; progress: Progress }
  | { kind: 'launching'; update: UpdateView }
  | { kind: 'error'; update: UpdateView; message: string };

/**
 * Estado da atualização: pergunta ao Rust se já há versão nova (o evento pode
 * ter saído antes da tela montar), escuta os eventos e dispara a instalação.
 */
export function useUpdate(): { state: UpdateState; install: () => void } {
  const [state, setState] = useState<UpdateState>({ kind: 'idle' });

  useEffect(() => {
    if (!isTauri()) {
      return;
    }
    let active = true;
    const offs: Array<() => void> = [];
    // Se a tela desmontar antes do registro terminar, desliga na hora.
    const keep = (off: () => void) => (active ? offs.push(off) : off());

    invoke<UpdateView | null>('update_status')
      .then((update) => {
        if (active && update) setState({ kind: 'available', update });
      })
      .catch(() => {});

    listen<UpdateView>('update://available', (e) => {
      setState((s) => (s.kind === 'idle' ? { kind: 'available', update: e.payload } : s));
    }).then(keep);

    listen<Progress>('update://progress', (e) => {
      setState((s) =>
        s.kind === 'downloading' ? { ...s, progress: e.payload } : s,
      );
    }).then(keep);

    return () => {
      active = false;
      offs.forEach((off) => off());
    };
  }, []);

  const install = useCallback(() => {
    if (state.kind !== 'available' && state.kind !== 'error') return;
    const update = state.update;
    setState({ kind: 'downloading', update, progress: { received: 0, total: null } });
    invoke('install_update')
      .then(() => setState({ kind: 'launching', update }))
      .catch((err: unknown) => setState({ kind: 'error', update, message: String(err) }));
  }, [state]);

  return { state, install };
}
