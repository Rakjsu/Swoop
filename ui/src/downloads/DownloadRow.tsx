import type { DownloadView } from '../gen/DownloadView';
import type { LiveRow } from '../gen/LiveRow';
import { displayName, formatBytes, formatEta, formatSpeed, progressOf, stateLabel } from './format';
import { FolderIcon, PauseIcon, PlayIcon, RetryIcon, TrashIcon } from './icons';

/** Altura fixa de cada linha (a lista virtual depende dela). */
export const ROW_HEIGHT = 56;

/** Ações que uma linha pode pedir. */
export type RowAction = 'pause' | 'resume' | 'retry' | 'remove' | 'reveal';

interface Props {
  row: DownloadView;
  live?: LiveRow;
  canReveal: boolean;
  onAction: (action: RowAction, row: DownloadView) => void;
}

/** Estados em que "pausar" vale (mesma regra de `DownloadState::next`). */
const PAUSABLE = new Set(['queued', 'resolving', 'downloading', 'verifying', 'waiting']);

/** Uma linha da lista: nome, tamanho, barra, velocidade, estado e ações. */
export function DownloadRow({ row, live, canReveal, onAction }: Props) {
  const { done, total } = progressOf(row, live);
  const pct = total ? Math.min(100, (done / total) * 100) : 0;
  const act = (action: RowAction) => () => onAction(action, row);

  return (
    <div className={`dl-row dl-${row.state}`} role="row">
      <div className="dl-name" role="cell">
        <span className="dl-title" title={row.url}>
          {displayName(row)}
        </span>
        <span className={row.error ? 'dl-sub dl-error' : 'dl-sub'}>
          {row.error ?? hostOf(row.url)}
        </span>
      </div>
      <div className="dl-size" role="cell">
        {total ? formatBytes(total) : '—'}
      </div>
      <div className="dl-progress" role="cell">
        <div className="dl-bar" aria-label={`${pct.toFixed(0)}%`}>
          <div className="dl-bar-fill" style={{ width: `${pct}%` }} />
        </div>
        <span className="dl-pct">{total ? `${pct.toFixed(1)}%` : formatBytes(done)}</span>
      </div>
      <div className="dl-speed" role="cell">
        {live && live.speed_bps > 0 ? formatSpeed(live.speed_bps) : ''}
      </div>
      <div className="dl-status" role="cell">
        {statusText(row, live)}
      </div>
      <div className="dl-actions" role="cell">
        {PAUSABLE.has(row.state) && (
          <IconButton label="Pausar" onClick={act('pause')}>
            <PauseIcon />
          </IconButton>
        )}
        {row.state === 'paused' && (
          <IconButton label="Retomar" onClick={act('resume')}>
            <PlayIcon />
          </IconButton>
        )}
        {row.state === 'failed' && (
          <IconButton label="Tentar de novo" onClick={act('retry')}>
            <RetryIcon />
          </IconButton>
        )}
        {canReveal && row.state === 'completed' && (
          <IconButton label="Mostrar na pasta" onClick={act('reveal')}>
            <FolderIcon />
          </IconButton>
        )}
        <IconButton label="Remover" onClick={act('remove')}>
          <TrashIcon />
        </IconButton>
      </div>
    </div>
  );
}

function IconButton(props: { label: string; onClick: () => void; children: React.ReactNode }) {
  return (
    <button type="button" className="icon-btn" title={props.label} aria-label={props.label} onClick={props.onClick}>
      {props.children}
    </button>
  );
}

/** Coluna de estado: tempo restante, contagem da espera ou o nome do estado. */
function statusText(row: DownloadView, live?: LiveRow): string {
  if (row.state === 'downloading' && live?.eta_secs != null) {
    return `${formatEta(live.eta_secs)} restantes`;
  }
  if (row.state === 'waiting' && row.wait_until_ms) {
    const secs = Math.max(0, Math.round((row.wait_until_ms - Date.now()) / 1000));
    return `Aguardando ${formatEta(secs)}`;
  }
  return stateLabel(row.state);
}

function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return '';
  }
}
