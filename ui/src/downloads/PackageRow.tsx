import { formatBytes, formatSpeed } from './format';
import type { PackageSummary } from './packages';

interface Props {
  pkg: PackageSummary;
  onToggle: (id: number) => void;
}

/** Cabeçalho de um pacote: recolhe/expande, soma o progresso dos arquivos. */
export function PackageRow({ pkg, onToggle }: Props) {
  const pct = pkg.total > 0 ? Math.min(100, (pkg.done / pkg.total) * 100) : 0;
  const files = pkg.count === 1 ? '1 arquivo' : `${pkg.count} arquivos`;
  const status =
    pkg.completed === pkg.count
      ? 'Concluído'
      : `${pkg.completed} de ${pkg.count} concluídos${pkg.failed ? `, ${pkg.failed} com falha` : ''}`;

  return (
    <div className="pkg-row dl-cols" role="row">
      <div className="dl-name" role="cell">
        <button
          type="button"
          className="pkg-toggle"
          aria-expanded={!pkg.collapsed}
          aria-label={`${pkg.collapsed ? 'Expandir' : 'Recolher'} ${pkg.name}`}
          onClick={() => onToggle(pkg.id)}
        >
          <span className="pkg-chevron" aria-hidden="true">
            {pkg.collapsed ? '▸' : '▾'}
          </span>
          <span className="pkg-name">{pkg.name}</span>
        </button>
        <span className="dl-sub">{files}</span>
      </div>
      <div className="dl-size" role="cell">
        {pkg.total ? formatBytes(pkg.total) : '—'}
      </div>
      <div className="dl-progress" role="cell">
        <div className="dl-bar" aria-label={`${pct.toFixed(0)}%`}>
          <div className="dl-bar-fill" style={{ width: `${pct}%` }} />
        </div>
        <span className="dl-pct">{pkg.total ? `${pct.toFixed(1)}%` : ''}</span>
      </div>
      <div className="dl-speed" role="cell">
        {pkg.speed > 0 ? formatSpeed(pkg.speed) : ''}
      </div>
      <div className="dl-status" role="cell">
        {status}
      </div>
      <div role="cell" />
    </div>
  );
}
