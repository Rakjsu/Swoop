import { useUpdate, type UpdateState } from './useUpdate';

/** Faixa no topo: "Versão X disponível — Atualizar", progresso ou erro. */
export function UpdateBanner() {
  const { state, install } = useUpdate();
  if (state.kind === 'idle') {
    return null;
  }
  return (
    <div className="update-banner" role="status">
      <span className="update-text">{message(state)}</span>
      {state.kind === 'downloading' && <ProgressBar state={state} />}
      {(state.kind === 'available' || state.kind === 'error') && (
        <button type="button" className="update-button" onClick={install}>
          {state.kind === 'error' ? 'Tentar de novo' : 'Atualizar'}
        </button>
      )}
    </div>
  );
}

/** Texto da faixa para cada estado. */
function message(state: Exclude<UpdateState, { kind: 'idle' }>): string {
  const v = state.update.version;
  switch (state.kind) {
    case 'available':
      return `Versão ${v} disponível.`;
    case 'downloading':
      return `Baixando a versão ${v}…`;
    case 'launching':
      return 'Instalando: o Swoop vai fechar e abrir de novo atualizado.';
    case 'error':
      return `Falha ao atualizar para ${v}: ${state.message}`;
  }
}

/** Barra de progresso do download do instalador. */
function ProgressBar({ state }: { state: Extract<UpdateState, { kind: 'downloading' }> }) {
  const { received, total } = state.progress;
  const pct = total ? Math.min(100, (received / total) * 100) : 0;
  return (
    <div className="update-progress" aria-label="progresso do download">
      <div className="update-progress-fill" style={{ width: `${pct}%` }} />
    </div>
  );
}
