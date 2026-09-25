import { useCallback, useState } from 'react';
import type { Command } from '../gen/Command';
import type { DownloadView } from '../gen/DownloadView';
import { errorMessage, type Transport } from '../transport';
import { AddLinksDialog } from './AddLinksDialog';
import { ConfirmRemove } from './ConfirmRemove';
import { DownloadRow, ROW_HEIGHT, type RowAction } from './DownloadRow';
import { StatusBar } from './StatusBar';
import { Toolbar } from './Toolbar';
import { useDownloads } from './useDownloads';
import { useVirtualRows } from './useVirtualRows';
import './downloads.css';

/** Tela principal: barra de ações, lista (virtual) de downloads e rodapé. */
export function DownloadsView({ transport }: { transport: Transport }) {
  const { rows, live, snapshot, loaded, error } = useDownloads(transport);
  const [adding, setAdding] = useState(false);
  const [removing, setRemoving] = useState<DownloadView | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const { ref: listRef, start, end, totalHeight, offset } = useVirtualRows(rows.length, ROW_HEIGHT);

  const run = useCallback(
    (cmd: Command) => {
      setActionError(null);
      transport.exec(cmd).catch((err: unknown) => setActionError(errorMessage(err)));
    },
    [transport],
  );

  const onAction = (action: RowAction, row: DownloadView) => {
    switch (action) {
      case 'pause':
      case 'resume':
      case 'retry':
        return run({ type: action, id: row.id });
      case 'remove':
        return setRemoving(row);
      case 'reveal':
        return void transport
          .revealDownload?.(row.id)
          .catch((err: unknown) => setActionError(errorMessage(err)));
    }
  };

  const alert = error ?? actionError;
  return (
    <section className="downloads">
      <Toolbar
        limitBps={snapshot?.limit_bps ?? null}
        onAdd={() => setAdding(true)}
        onPauseAll={() => run({ type: 'pause_all' })}
        onResumeAll={() => run({ type: 'resume_all' })}
        onLimit={(bps) => run({ type: 'set_speed_limit', ...(bps ? { bps } : {}) })}
      />
      {alert && (
        <div className="alert" role="alert">
          <span>{alert}</span>
          {actionError && (
            <button type="button" className="alert-close" aria-label="Fechar" onClick={() => setActionError(null)}>
              ×
            </button>
          )}
        </div>
      )}
      <div className="dl-head dl-cols" role="row">
        <span>Arquivo</span>
        <span>Tamanho</span>
        <span>Progresso</span>
        <span>Velocidade</span>
        <span>Estado</span>
        <span />
      </div>
      <div className="dl-list" ref={listRef} role="table" aria-label="Downloads">
        {loaded && rows.length === 0 ? (
          <div className="dl-empty">
            <p>Nenhum download ainda.</p>
            <button type="button" className="btn btn-primary" onClick={() => setAdding(true)}>
              Adicionar links
            </button>
          </div>
        ) : (
          <div className="dl-spacer" style={{ height: totalHeight }}>
            <div className="dl-window" style={{ transform: `translateY(${offset}px)` }}>
              {rows.slice(start, end).map((row) => (
                <DownloadRow
                  key={row.id}
                  row={row}
                  live={live.get(row.id)}
                  canReveal={Boolean(transport.revealDownload)}
                  onAction={onAction}
                />
              ))}
            </div>
          </div>
        )}
      </div>
      <StatusBar rows={rows} snapshot={snapshot} />
      {adding && <AddLinksDialog transport={transport} onClose={() => setAdding(false)} />}
      {removing && (
        <ConfirmRemove
          row={removing}
          onCancel={() => setRemoving(null)}
          onConfirm={(deleteFile) => {
            run({ type: 'remove', id: removing.id, delete_file: deleteFile });
            setRemoving(null);
          }}
        />
      )}
    </section>
  );
}
