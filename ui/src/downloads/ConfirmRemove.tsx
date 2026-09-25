import { useState } from 'react';
import type { DownloadView } from '../gen/DownloadView';
import { displayName } from './format';

interface Props {
  row: DownloadView;
  onConfirm: (deleteFile: boolean) => void;
  onCancel: () => void;
}

/**
 * Confirmação de remoção. Incompleto: o progresso (o `.part`) é apagado.
 * Concluído: o arquivo fica, a menos que se marque "apagar também".
 */
export function ConfirmRemove({ row, onConfirm, onCancel }: Props) {
  const [deleteFile, setDeleteFile] = useState(false);
  const done = row.state === 'completed';
  return (
    <div className="modal-backdrop" role="presentation" onClick={onCancel}>
      <div
        className="modal modal-small"
        role="dialog"
        aria-modal="true"
        aria-labelledby="remove-title"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 id="remove-title">Remover da lista?</h2>
        <p className="modal-text">{displayName(row)}</p>
        {done ? (
          <label className="modal-check">
            <input type="checkbox" checked={deleteFile} onChange={(e) => setDeleteFile(e.target.checked)} />
            Apagar também o arquivo baixado
          </label>
        ) : (
          <p className="modal-text dim">O que já foi baixado deste arquivo será descartado.</p>
        )}
        <div className="modal-actions">
          <button type="button" className="btn" onClick={onCancel}>
            Cancelar
          </button>
          <button type="button" className="btn btn-danger" onClick={() => onConfirm(done && deleteFile)}>
            Remover
          </button>
        </div>
      </div>
    </div>
  );
}
