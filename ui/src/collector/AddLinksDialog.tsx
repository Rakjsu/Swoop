import { useState } from 'react';
import { errorMessage, type Transport } from '../transport';

interface Props {
  transport: Transport;
  onClose: () => void;
  /** Chamado depois que os links entraram no coletor. */
  onAdded?: () => void;
}

/** Links http(s) do texto colado (só para contar; o backend acha os demais). */
export function parseLinks(text: string): string[] {
  const links = text.split(/\s+/).filter((part) => /^https?:\/\/\S+$/i.test(part));
  return [...new Set(links)];
}

/**
 * Janela "Adicionar links": o texto colado vai para o coletor, que acha os
 * links (inclusive no meio de frases), abre as pastas e confere cada um.
 */
export function AddLinksDialog({ transport, onClose, onAdded }: Props) {
  const [text, setText] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const links = parseLinks(text);

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      await transport.exec({ type: 'collect', text });
      onClose();
      onAdded?.();
    } catch (err) {
      setError(errorMessage(err));
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop" role="presentation" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="add-title"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 id="add-title">Adicionar links</h2>
        <textarea
          autoFocus
          rows={8}
          placeholder="Cole os links (ou um texto com links). Pastas do Drive e do Mediafire viram os arquivos."
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
        <p className="modal-hint">Os links vão para o Coletor, que confere se estão online antes de baixar.</p>
        {error && <p className="modal-error">{error}</p>}
        <div className="modal-actions">
          <button type="button" className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button
            type="button"
            className="btn btn-primary"
            disabled={busy || text.trim() === ''}
            onClick={submit}
          >
            {links.length > 1 ? `Adicionar ${links.length} links` : 'Adicionar'}
          </button>
        </div>
      </div>
    </div>
  );
}
