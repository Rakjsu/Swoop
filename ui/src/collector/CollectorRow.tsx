import type { CollectorView } from '../gen/CollectorView';
import type { LinkState } from '../gen/LinkState';
import { displayName, formatBytes } from '../downloads/format';

const LABELS: Record<LinkState, string> = {
  unchecked: 'Na vez',
  checking: 'Verificando…',
  online: 'Online',
  offline: 'Offline',
  failed: 'Não verificado',
};

interface Props {
  row: CollectorView;
  selected: boolean;
  onToggle: (id: number) => void;
}

/** Uma linha do coletor: seleção, estado, nome, servidor e tamanho. */
export function CollectorRow({ row, selected, onToggle }: Props) {
  return (
    <div className={`col-row col-${row.state}`} role="row">
      <input
        type="checkbox"
        aria-label={`Selecionar ${displayName(row)}`}
        checked={selected}
        onChange={() => onToggle(row.id)}
      />
      <span className="col-state" role="cell">
        {LABELS[row.state]}
      </span>
      <div className="col-name" role="cell">
        <span className="col-title" title={row.url}>
          {displayName(row)}
        </span>
        {row.error && <span className="col-error">{row.error}</span>}
      </div>
      <span className="col-host" role="cell">
        {row.host}
      </span>
      <span className="col-size" role="cell">
        {row.size != null ? formatBytes(row.size) : '—'}
      </span>
    </div>
  );
}
