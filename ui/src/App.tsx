import { useEffect, useState } from 'react';
import { loadAppInfo, type AppInfo } from './appInfo';
import { AccountsView } from './accounts/AccountsView';
import { CollectorView } from './collector/CollectorView';
import { DownloadsView } from './downloads/DownloadsView';
import { HistoryView } from './history/HistoryView';
import { OptionsView } from './options/OptionsView';
import { transport, type Transport } from './transport';
import { UpdateBanner } from './update/UpdateBanner';

type Tab = 'downloads' | 'collector' | 'history' | 'accounts' | 'options';

/** Abas deste ambiente: Contas só no app do computador. */
const TABS: Array<[Tab, string]> = [
  ['downloads', 'Downloads'],
  ['collector', 'Coletor'],
  ['history', 'Histórico'],
  ...(transport?.accounts ? [['accounts', 'Contas'] as [Tab, string]] : []),
  ['options', 'Opções'],
];

/** Casca do app: faixa de atualização, cabeçalho com abas e a tela escolhida. */
export function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [tab, setTab] = useState<Tab>('downloads');

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
        {transport && (
          <nav className="tabs" role="tablist">
            {TABS.map(([id, label]) => (
              <button
                key={id}
                type="button"
                role="tab"
                aria-selected={tab === id}
                className={tab === id ? 'tab tab-active' : 'tab'}
                onClick={() => setTab(id)}
              >
                {label}
              </button>
            ))}
          </nav>
        )}
      </header>
      {transport ? <Screen tab={tab} transport={transport} go={setTab} /> : <BrowserNotice />}
    </div>
  );
}

function Screen({ tab, transport, go }: { tab: Tab; transport: Transport; go: (tab: Tab) => void }) {
  switch (tab) {
    case 'downloads':
      return <DownloadsView transport={transport} onCollected={() => go('collector')} />;
    case 'collector':
      return <CollectorView transport={transport} onStarted={() => go('downloads')} />;
    case 'history':
      return <HistoryView transport={transport} />;
    case 'accounts':
      return <AccountsView transport={transport} />;
    case 'options':
      return <OptionsView transport={transport} />;
  }
}

/** Aberto num navegador sem o endereço do painel (falta o token). */
function BrowserNotice() {
  return (
    <main className="splash">
      <img className="splash-logo" src="/logo.svg" alt="" width={96} height={96} />
      <h1 className="splash-title">Swoop</h1>
      <p className="splash-tagline">Seus downloads, num mergulho.</p>
      <p className="splash-status">
        Para usar no navegador, abra o endereço completo que o <code>swoop-cli serve</code> mostra.
      </p>
    </main>
  );
}
