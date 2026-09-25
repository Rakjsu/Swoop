import { isTauri } from '@tauri-apps/api/core';
import type { Command } from '../gen/Command';
import type { DownloadView } from '../gen/DownloadView';
import type { Push } from '../gen/Push';
import type { Reply } from '../gen/Reply';
import { tauriTransport } from './tauri';

/**
 * Como a UI fala com o motor. A janela desktop usa o IPC do Tauri; o painel
 * no navegador (v0.3.0) terá a mesma interface sobre HTTP + WebSocket.
 */
export interface Transport {
  exec(cmd: Command): Promise<Reply>;
  list(): Promise<DownloadView[]>;
  /** Registra um ouvinte de retratos e avisos; devolve a função que desliga. */
  onPush(listener: (push: Push) => void): () => void;
  /** Mostra o arquivo no Explorer (só no desktop). */
  revealDownload?(id: number): Promise<void>;
  /** Janela "Escolher pasta" do sistema (só no desktop). */
  pickFolder?(): Promise<string | null>;
}

/** Transporte deste ambiente; `null` no navegador enquanto o painel não existe. */
export const transport: Transport | null = isTauri() ? tauriTransport : null;

/** Mensagem legível de um erro do backend (`ApiError`) ou de qualquer outro. */
export function errorMessage(err: unknown): string {
  if (typeof err === 'object' && err !== null && 'message' in err) {
    const { message } = err as { message: unknown };
    if (typeof message === 'string') return message;
  }
  return String(err);
}
