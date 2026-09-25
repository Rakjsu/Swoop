import type { DownloadState } from '../gen/DownloadState';
import type { DownloadView } from '../gen/DownloadView';
import type { LiveRow } from '../gen/LiveRow';

const UNITS = ['B', 'KB', 'MB', 'GB', 'TB'];

/** Tamanho legível em base 1024 ("12,4 MB"). */
export function formatBytes(bytes: number): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 || value >= 100 ? 0 : 1;
  return `${value.toLocaleString('pt-BR', { maximumFractionDigits: digits })} ${UNITS[unit]}`;
}

/** Velocidade legível ("3,2 MB/s"). */
export function formatSpeed(bps: number): string {
  return `${formatBytes(bps)}/s`;
}

/** Tempo restante curto ("1 h 05 min", "42 s"). */
export function formatEta(secs: number): string {
  if (secs < 60) return `${secs} s`;
  const min = Math.floor(secs / 60);
  if (min < 60) return `${min} min ${String(secs % 60).padStart(2, '0')} s`;
  return `${Math.floor(min / 60)} h ${String(min % 60).padStart(2, '0')} min`;
}

const LABELS: Record<DownloadState, string> = {
  queued: 'Na fila',
  resolving: 'Preparando',
  downloading: 'Baixando',
  verifying: 'Conferindo',
  downloaded: 'Finalizando',
  completed: 'Concluído',
  waiting: 'Aguardando',
  paused: 'Pausado',
  failed: 'Falhou',
};

/** Nome do estado para o usuário. */
export function stateLabel(state: DownloadState): string {
  return LABELS[state];
}

/** Nome exibido: o do arquivo, ou o fim do link enquanto não se sabe. */
export function displayName(row: Pick<DownloadView, 'file_name' | 'url'>): string {
  if (row.file_name) return row.file_name;
  try {
    const path = new URL(row.url).pathname.split('/').filter(Boolean);
    return decodeURIComponent(path[path.length - 1] ?? row.url);
  } catch {
    return row.url;
  }
}

/** Bytes recebidos e total, com o valor ao vivo por cima do banco. */
export function progressOf(row: DownloadView, live?: LiveRow): { done: number; total: number | null } {
  if (row.state === 'completed') {
    const size = row.size ?? row.done_bytes;
    return { done: size, total: size };
  }
  return {
    done: live?.received ?? row.done_bytes,
    total: live?.total ?? row.size,
  };
}
