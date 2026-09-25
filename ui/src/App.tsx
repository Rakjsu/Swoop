import { useEffect, useState } from 'react';
import { loadAppInfo, type AppInfo } from './appInfo';
import { DownloadsView } from './downloads/DownloadsView';
import { transport } from './transport';
import { UpdateBanner } from './update/UpdateBanner';

/** Casca do app: faixa de atualização, cabeçalho e a tela de downloads. */
export function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    loadAppInfo()
      .then(setInfo)
      .catch((err: unknown) => console.error('app_info falhou', err));
  }, []);

  return (
    <div className="app">
      <UpdateBanner />
      <header className="app-header">
        <img src="/logo.svg" alt="" width={26} height={26} />
        <span className="app-title">Swoop</span>
        {info && <span className="app-version">v{info.version}</span>}
      </header>
      {transport ? <DownloadsView transport={transport} /> : <BrowserNotice />}
    </div>
  );
}

/** Aberto num navegador comum: ainda não há painel web. */
function BrowserNotice() {
  return (
    <main className="splash">
      <img className="splash-logo" src="/logo.svg" alt="" width={96} height={96} />
      <h1 className="splash-title">Swoop</h1>
      <p className="splash-tagline">Seus downloads, num mergulho.</p>
      <p className="splash-status">Abra pelo app instalado: o painel no navegador chega na v0.3.0.</p>
    </main>
  );
}
