import { isTauri } from '@tauri-apps/api/core';
import type { Command } from '../gen/Command';
import type { DownloadView } from '../gen/DownloadView';
import type { HistoryView } from '../gen/HistoryView';
import type { Push } from '../gen/Push';
import type { Reply } from '../gen/Reply';
import type { Settings } from '../gen/Settings';
import { createHttpTransport } from './http';
import { tauriTransport } from './tauri';

/**
 * Como a UI fala com o motor. A janela desktop usa o IPC do Tauri; o painel
 * no navegador (`swoop-cli serve`) usa HTTP + WebSocket com o token do `#t=`.
 */
export interface Transport {
  exec(cmd: Command): Promise<Reply>;
  list(): Promise<DownloadView[]>;
  history(limit: number): Promise<HistoryView[]>;
  settings(): Promise<Settings>;
  /** Registra um ouvinte de retratos e avisos; devolve a função que desliga. */
  onPush(listener: (push: Push) => void): () => void;
  /** Mostra o arquivo no Explorer (só no desktop). */
  revealDownload?(id: number): Promise<void>;
  /** Janela "Escolher pasta" do sistema (só no desktop). */
  pickFolder?(): Promise<string | null>;
}

/** Transporte deste ambiente; `null` no navegador sem o token do painel. */
export const transport: Transport | null = isTauri() ? tauriTransport : createHttpTransport();

/** Mensagem legível de um erro do backend (`ApiError`) ou de qualquer outro. */
export function errorMessage(err: unknown): string {
  if (typeof err === 'object' && err !== null && 'message' in err) {
    const { message } = err as { message: unknown };
    if (typeof message === 'string') return message;
  }
  return String(err);
}
