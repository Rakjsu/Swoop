import { useState } from 'react';
import type { CollectorView as Link } from '../gen/CollectorView';
import type { Command } from '../gen/Command';
import type { Settings } from '../gen/Settings';
import { errorMessage, type Transport } from '../transport';
import { useLiveQuery } from '../transport/useLiveQuery';
import { PlusIcon } from '../downloads/icons';
import { AddLinksDialog } from './AddLinksDialog';
import { CollectorRow } from './CollectorRow';
import './collector.css';

const NO_LINKS: Link[] = [];
const listLinks = (t: Transport) => t.collector();
const loadSettings = (t: Transport): Promise<Settings | null> => t.settings();

interface Props {
  transport: Transport;
  /** Vai para a aba Downloads depois de iniciar. */
  onStarted: () => void;
}

/**
 * Coletor (como o do Mipony): os links colados são conferidos no servidor
 * (online, nome, tamanho) e só vão para a fila no "Iniciar". Com "Iniciar
 * sozinho", cada colagem vai para a fila assim que termina de ser conferida.
 */
export function CollectorView({ transport, onStarted }: Props) {
  const { data: links, loaded, error } = useLiveQuery(transport, listLinks, NO_LINKS);
  const { data: settings } = useLiveQuery(transport, loadSettings, null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [adding, setAdding] = useState(false);
  const [dest, setDest] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  // "Iniciar sozinho" responde na hora; volta atrás se não salvar.
  const [autoStart, setAutoStart] = useState<boolean | null>(null);

  const run = (cmd: Command, then?: () => void, onError?: () => void) => {
    setActionError(null);
    transport
      .exec(cmd)
      .then(() => then?.())
      .catch((err: unknown) => {
        onError?.();
        setActionError(errorMessage(err));
      });
  };
  const setAuto = (settings: Settings, on: boolean) => {
    setAutoStart(on);
    run({ type: 'save_settings', settings: { ...settings, auto_start: on } }, undefined, () => setAutoStart(null));
  };
  const start = (ids: number[]) =>
    run({ type: 'collector_start', ids, ...(dest ? { dest } : {}) }, () => {
      setSelected(new Set());
      onStarted();
    });
  const toggle = (id: number) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  const pick = () =>
    transport
      .pickFolder?.()
      .then((dir) => dir && setDest(dir))
      .catch((err: unknown) => setActionError(errorMessage(err)));

  const online = links.filter((l) => l.state === 'online').map((l) => l.id);
  const chosen = links.filter((l) => selected.has(l.id)).map((l) => l.id);
  const offline = links.some((l) => l.state === 'offline');
  const alert = error ?? actionError;

  return (
    <section className="collector">
      <div className="toolbar">
        <button type="button" className="btn" onClick={() => setAdding(true)}>
          <PlusIcon /> Adicionar links
        </button>
        <button type="button" className="btn btn-primary" disabled={online.length === 0} onClick={() => start(online)}>
          {`Iniciar todos online (${online.length})`}
        </button>
        <button type="button" className="btn" disabled={chosen.length === 0} onClick={() => start(chosen)}>
          {`Iniciar selecionados (${chosen.length})`}
        </button>
        <button type="button" className="btn" disabled={!offline} onClick={() => run({ type: 'collector_remove_offline' })}>
          Remover offline
        </button>
        <button type="button" className="btn" disabled={links.length === 0} onClick={() => run({ type: 'collector_clear' })}>
          Limpar
        </button>
        {settings && (
          <label className="col-auto">
            <input
              type="checkbox"
              checked={autoStart ?? settings.auto_start}
              onChange={(e) => setAuto(settings, e.target.checked)}
            />
            Iniciar sozinho
          </label>
        )}
      </div>
      <div className="col-dest">
        <span title={dest ?? undefined}>{dest ?? 'Pasta automática: vídeos, músicas e outros, cada um na sua'}</span>
        {transport.pickFolder && (
          <button type="button" className="btn" onClick={pick}>
            Escolher pasta…
          </button>
        )}
        {dest && (
          <button type="button" className="btn" onClick={() => setDest(null)}>
            Automática
          </button>
        )}
      </div>
      {alert && (
        <div className="alert" role="alert">
          <span>{alert}</span>
        </div>
      )}
      <div className="col-list" role="table" aria-label="Coletor">
        {loaded && links.length === 0 ? (
          <div className="dl-empty">
            <p>Cole links para conferir antes de baixar.</p>
            <button type="button" className="btn btn-primary" onClick={() => setAdding(true)}>
              Adicionar links
            </button>
          </div>
        ) : (
          links.map((link) => (
            <CollectorRow key={link.id} row={link} selected={selected.has(link.id)} onToggle={toggle} />
          ))
        )}
      </div>
      {adding && <AddLinksDialog transport={transport} onClose={() => setAdding(false)} />}
    </section>
  );
}
