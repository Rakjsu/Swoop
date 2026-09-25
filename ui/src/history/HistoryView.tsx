import type { HistoryOutcome } from '../gen/HistoryOutcome';
import type { HistoryView as Entry } from '../gen/HistoryView';
import { displayName, formatBytes, formatSpeed } from '../downloads/format';
import { useVirtualRows } from '../downloads/useVirtualRows';
import type { Transport } from '../transport';
import { useLiveQuery } from '../transport/useLiveQuery';
import './history.css';

/** Entradas mostradas (as mais recentes). */
const LIMIT = 500;
const ROW_HEIGHT = 48;
const NO_ENTRIES: Entry[] = [];
const loadHistory = (t: Transport) => t.history(LIMIT);

const OUTCOMES: Record<HistoryOutcome, string> = {
  completed: 'Concluído',
  failed: 'Falhou',
  removed: 'Removido',
};

const WHEN = new Intl.DateTimeFormat('pt-BR', { dateStyle: 'short', timeStyle: 'short' });

/** Tela de histórico: o que terminou, falhou ou foi removido. */
export function HistoryView({ transport }: { transport: Transport }) {
  const { data: rows, loaded, error } = useLiveQuery(transport, loadHistory, NO_ENTRIES);
  const { ref, start, end, totalHeight, offset } = useVirtualRows(rows.length, ROW_HEIGHT);

  return (
    <section className="history">
      {error && (
        <div className="alert" role="alert">
          <span>{error}</span>
        </div>
      )}
      <div className="hist-row hist-head" role="row">
        <span>Arquivo</span>
        <span>Tamanho</span>
        <span>Resultado</span>
        <span>Quando</span>
        <span>Média</span>
      </div>
      <div className="hist-list" ref={ref} role="table" aria-label="Histórico">
        {loaded && rows.length === 0 ? (
          <p className="hist-empty">Nada no histórico ainda.</p>
        ) : (
          <div style={{ height: totalHeight, position: 'relative' }}>
            <div style={{ transform: `translateY(${offset}px)` }}>
              {rows.slice(start, end).map((row) => (
                <HistoryRow key={row.id} row={row} />
              ))}
            </div>
          </div>
        )}
      </div>
    </section>
  );
}

function HistoryRow({ row }: { row: Entry }) {
  const name = displayName({ file_name: row.file_name, url: row.url });
  return (
    <div className={`hist-row hist-${row.outcome}`} role="row">
      <div className="hist-name" role="cell">
        <span title={row.final_path ?? row.url}>{name}</span>
        {row.error && <span className="hist-error">{row.error}</span>}
      </div>
      <span role="cell">{row.size != null ? formatBytes(row.size) : '—'}</span>
      <span role="cell" className="hist-outcome">
        {OUTCOMES[row.outcome]}
      </span>
      <span role="cell">{WHEN.format(new Date(row.finished_ms))}</span>
      <span role="cell">{row.avg_bps ? formatSpeed(row.avg_bps) : '—'}</span>
    </div>
  );
}
