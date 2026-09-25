import { useState } from 'react';
import { errorMessage, type Transport } from '../transport';

interface Props {
  transport: Transport;
  onClose: () => void;
}

/** Links http(s) do texto colado (um por linha, espaços também separam). */
export function parseLinks(text: string): string[] {
  const links = text.split(/\s+/).filter((part) => /^https?:\/\/\S+$/i.test(part));
  return [...new Set(links)];
}

/** Janela "Adicionar links": cola os links, escolhe a pasta e manda baixar. */
export function AddLinksDialog({ transport, onClose }: Props) {
  const [text, setText] = useState('');
  const [dest, setDest] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const links = parseLinks(text);

  const pick = async () => {
    try {
      const dir = await transport.pickFolder?.();
      if (dir) setDest(dir);
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      await transport.exec({ type: 'add_links', links, ...(dest ? { dest } : {}) });
      onClose();
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
          placeholder="Cole um link por linha (http:// ou https://)"
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
        <div className="modal-dest">
          <span className="modal-dest-path" title={dest ?? undefined}>
            {dest ?? 'Pasta padrão (Downloads\\Swoop)'}
          </span>
          {transport.pickFolder && (
            <button type="button" className="btn" onClick={pick}>
              Escolher pasta…
            </button>
          )}
        </div>
        {error && <p className="modal-error">{error}</p>}
        <div className="modal-actions">
          <button type="button" className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button
            type="button"
            className="btn btn-primary"
            disabled={busy || links.length === 0}
            onClick={submit}
          >
            {links.length > 1 ? `Adicionar ${links.length} links` : 'Adicionar'}
          </button>
        </div>
      </div>
    </div>
  );
}
