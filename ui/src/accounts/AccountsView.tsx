import { useState } from 'react';
import type { AccountView } from '../gen/AccountView';
import type { AccountsView as Data } from '../gen/AccountsView';
import { formatBytes } from '../downloads/format';
import { errorMessage, type Transport } from '../transport';
import { useLiveQuery } from '../transport/useLiveQuery';
import { AccountForm } from './AccountForm';
import './accounts.css';

const EMPTY: Data = { hosts: [], accounts: [] };
const DAY = new Intl.DateTimeFormat('pt-BR', { dateStyle: 'short' });
const loadAccounts = (t: Transport) => (t.accounts ? t.accounts() : Promise.resolve(EMPTY));

/** Situação da conta para o usuário. */
export function statusText(a: AccountView): string {
  switch (a.status) {
    case 'premium':
      return a.premium_until_ms ? `Premium até ${DAY.format(a.premium_until_ms)}` : 'Premium';
    case 'free':
      return 'Sem premium (ou vencido)';
    case 'invalid':
      return 'Recusada pelo site';
    case 'error':
      return 'Não deu para conferir';
    case 'unchecked':
      return 'Conferindo…';
  }
}

/** Aba Contas: contas premium do usuário, uma por servidor. */
export function AccountsView({ transport }: { transport: Transport }) {
  const { data, error } = useLiveQuery(transport, loadAccounts, EMPTY);
  const [actionError, setActionError] = useState<string | null>(null);
  const run = (type: 'check_account' | 'remove_account', host: string) => {
    setActionError(null);
    transport.exec({ type, host }).catch((err: unknown) => setActionError(errorMessage(err)));
  };

  return (
    <section className="accounts">
      <h2>Adicionar conta</h2>
      <p className="acc-hint">
        A senha ou a chave fica no cofre do sistema (no Windows, o Gerenciador de Credenciais), nunca no banco do
        Swoop. Com conta premium, o download não tem contador nem captcha e usa várias conexões.
      </p>
      <AccountForm transport={transport} hosts={data.hosts} />
      {(error ?? actionError) && (
        <div className="alert" role="alert">
          <span>{error ?? actionError}</span>
        </div>
      )}
      <h2>Contas</h2>
      {data.accounts.length === 0 ? (
        <p className="acc-hint">Nenhuma conta cadastrada.</p>
      ) : (
        <ul className="acc-list">
          {data.accounts.map((a) => (
            <li key={a.host} className={`acc-row acc-st-${a.status}`}>
              <div className="acc-who">
                <strong>{a.host}</strong>
                <span>{a.kind === 'api_key' ? 'chave da API' : a.username}</span>
              </div>
              <div className="acc-status">
                <span>{statusText(a)}</span>
                {a.traffic_left != null && <span>{formatBytes(a.traffic_left)} de tráfego</span>}
                {a.error && <span className="acc-error">{a.error}</span>}
              </div>
              <button type="button" className="btn" onClick={() => run('check_account', a.host)}>
                Conferir
              </button>
              <button type="button" className="btn" onClick={() => run('remove_account', a.host)}>
                Remover
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
