import { useCallback, useMemo, useState } from 'react';
import { AddLinksDialog } from '../collector/AddLinksDialog';
import type { Command } from '../gen/Command';
import type { DownloadView } from '../gen/DownloadView';
import { errorMessage, type Transport } from '../transport';
import { ConfirmRemove } from './ConfirmRemove';
import { DownloadRow, ROW_HEIGHT, type RowAction } from './DownloadRow';
import { PackageRow } from './PackageRow';
import { buildItems } from './packages';
import { StatusBar } from './StatusBar';
import { Toolbar } from './Toolbar';
import { useCollapsed } from './useCollapsed';
import { useDownloads } from './useDownloads';
import { useVirtualRows } from './useVirtualRows';
import './downloads.css';

interface Props {
  transport: Transport;
  /** Vai para o Coletor depois de adicionar links. */
  onCollected: () => void;
}

/**
 * Tela principal: barra de ações, lista (virtual) de downloads agrupados em
 * pacotes recolhíveis e rodapé.
 */
export function DownloadsView({ transport, onCollected }: Props) {
  const { rows, live, snapshot, loaded, error } = useDownloads(transport);
  const [collapsed, toggleCollapsed] = useCollapsed();
  const items = useMemo(() => buildItems(rows, live, collapsed), [rows, live, collapsed]);
  const [adding, setAdding] = useState(false);
  const [removing, setRemoving] = useState<DownloadView | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const { ref: listRef, start, end, totalHeight, offset } = useVirtualRows(items.length, ROW_HEIGHT);

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
      case 'captcha':
        return void transport
          .solveCaptcha?.(row.id)
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
              {items.slice(start, end).map((item) =>
                item.kind === 'package' ? (
                  <PackageRow key={item.key} pkg={item.pkg} onToggle={toggleCollapsed} />
                ) : (
                  <DownloadRow
                    key={item.key}
                    row={item.row}
                    live={live.get(item.row.id)}
                    canReveal={Boolean(transport.revealDownload)}
                    canSolve={Boolean(transport.solveCaptcha)}
                    onAction={onAction}
                  />
                ),
              )}
            </div>
          </div>
        )}
      </div>
      <StatusBar rows={rows} snapshot={snapshot} />
      {adding && <AddLinksDialog transport={transport} onClose={() => setAdding(false)} onAdded={onCollected} />}
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
