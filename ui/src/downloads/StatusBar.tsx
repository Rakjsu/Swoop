import type { DownloadView } from '../gen/DownloadView';
import type { Snapshot } from '../gen/Snapshot';
import { formatSpeed } from './format';

interface Props {
  rows: DownloadView[];
  snapshot: Snapshot | null;
}

/** Rodapé: velocidade total, quantos baixando/na fila/concluídos e o limite. */
export function StatusBar({ rows, snapshot }: Props) {
  const count = (state: DownloadView['state']) => rows.filter((r) => r.state === state).length;
  const active = snapshot?.active.length ?? 0;
  const limit = snapshot?.limit_bps;
  return (
    <footer className="statusbar" role="status">
      <span className="statusbar-speed">↓ {formatSpeed(snapshot?.speed_bps ?? 0)}</span>
      <span>{active} baixando</span>
      <span>{count('queued')} na fila</span>
      <span>{count('completed')} concluídos</span>
      {count('failed') > 0 && <span className="statusbar-failed">{count('failed')} com falha</span>}
      <span className="statusbar-limit">Limite: {limit ? formatSpeed(limit) : 'sem limite'}</span>
    </footer>
  );
}
