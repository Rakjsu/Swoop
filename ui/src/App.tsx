import { useEffect, useState } from 'react';
import { loadAppInfo, type AppInfo } from './appInfo';
import { UpdateBanner } from './update/UpdateBanner';

type InfoState =
  | { kind: 'loading' }
  | { kind: 'ready'; info: AppInfo }
  | { kind: 'browser' }
  | { kind: 'error'; message: string };

/** Tela inicial da fase 0: confirma que UI e processo Rust conversam. */
export function App() {
  const [state, setState] = useState<InfoState>({ kind: 'loading' });

  useEffect(() => {
    loadAppInfo()
      .then((info) => setState(info ? { kind: 'ready', info } : { kind: 'browser' }))
      .catch((err: unknown) =>
        setState({ kind: 'error', message: err instanceof Error ? err.message : String(err) }),
      );
  }, []);

  return (
    <>
      <UpdateBanner />
      <main className="splash">
        <img className="splash-logo" src="/logo.svg" alt="" width={112} height={112} />
        <h1 className="splash-title">Swoop</h1>
        <p className="splash-tagline">Seus downloads, num mergulho.</p>
        <p className="splash-status" role="status">
          {statusText(state)}
        </p>
      </main>
    </>
  );
}

/** Texto de estado exibido abaixo do slogan. */
function statusText(state: InfoState): string {
  switch (state.kind) {
    case 'loading':
      return 'Conectando ao motor…';
    case 'ready':
      return `Versão ${state.info.version}`;
    case 'browser':
      return 'Aberto no navegador: o painel remoto chega na fase 2.';
    case 'error':
      return `Falha ao falar com o motor: ${state.message}`;
  }
}
