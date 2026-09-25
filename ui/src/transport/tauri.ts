import { Channel, invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import type { Push } from '../gen/Push';
import type { Transport } from './index';

const listeners = new Set<(push: Push) => void>();
let subscribed = false;

/**
 * Assina uma vez por carga da página: o Rust mantém só a última assinatura,
 * e duas chamadas concorrentes (StrictMode) poderiam derrubar a que vale.
 * Os componentes ouvem este emissor local.
 */
function ensureSubscribed(): void {
  if (subscribed) return;
  subscribed = true;
  const channel = new Channel<Push>();
  channel.onmessage = (push) => listeners.forEach((listener) => listener(push));
  invoke('subscribe', { onPush: channel }).catch((err: unknown) => {
    subscribed = false;
    console.error('não consegui assinar o progresso', err);
  });
}

/** Transporte da janela desktop (IPC do Tauri). */
export const tauriTransport: Transport = {
  exec: (cmd) => invoke('exec', { cmd }),
  list: () => invoke('list'),
  history: (limit) => invoke('history', { limit }),
  collector: () => invoke('collector'),
  settings: () => invoke('settings'),
  onPush(listener) {
    listeners.add(listener);
    ensureSubscribed();
    return () => {
      listeners.delete(listener);
    };
  },
  revealDownload: (id) => invoke('reveal_download', { id }),
  async pickFolder() {
    const dir = await open({ directory: true, title: 'Pasta de destino' });
    return typeof dir === 'string' ? dir : null;
  },
};
