import { useState } from 'react';
import type { AccountHost } from '../gen/AccountHost';
import type { AccountKind } from '../gen/AccountKind';
import { errorMessage, type Transport } from '../transport';

interface Props {
  transport: Transport;
  hosts: AccountHost[];
}

const KIND_LABEL: Record<AccountKind, string> = {
  login: 'Usuário (ou e-mail) e senha',
  api_key: 'Chave da API',
};

/**
 * Cadastro de conta. A senha/chave vai direto para o cofre do sistema pelo
 * backend e sai do formulário assim que é enviada.
 */
export function AccountForm({ transport, hosts }: Props) {
  const [host, setHost] = useState('');
  const [kind, setKind] = useState<AccountKind>('login');
  const [username, setUsername] = useState('');
  const [secret, setSecret] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const chosen = host || hosts[0]?.host || '';
  // Só os tipos que o servidor aceita (o Rapidgator só usa e-mail e senha).
  const kinds = hosts.find((h) => h.host === chosen)?.kinds ?? [];
  const usedKind: AccountKind = kinds.includes(kind) ? kind : (kinds[0] ?? kind);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    const value = secret;
    setSecret('');
    setBusy(true);
    setError(null);
    try {
      await transport.exec({ type: 'add_account', host: chosen, kind: usedKind, username, secret: value });
      setUsername('');
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form className="acc-form" onSubmit={(e) => void submit(e)}>
      <label className="acc-field">
        <span>Servidor</span>
        <select value={chosen} onChange={(e) => setHost(e.target.value)}>
          {hosts.map((h) => (
            <option key={h.host} value={h.host}>
              {h.host}
            </option>
          ))}
        </select>
      </label>
      <label className="acc-field">
        <span>Tipo</span>
        <select value={usedKind} onChange={(e) => setKind(e.target.value as AccountKind)}>
          {kinds.map((k) => (
            <option key={k} value={k}>
              {KIND_LABEL[k]}
            </option>
          ))}
        </select>
      </label>
      {usedKind === 'login' && (
        <label className="acc-field">
          <span>Usuário</span>
          <input type="text" autoComplete="off" value={username} onChange={(e) => setUsername(e.target.value)} />
        </label>
      )}
      <label className="acc-field">
        <span>{usedKind === 'login' ? 'Senha' : 'Chave'}</span>
        <input type="password" autoComplete="off" value={secret} onChange={(e) => setSecret(e.target.value)} />
      </label>
      <button type="submit" className="btn btn-primary" disabled={busy || !chosen || !secret}>
        {busy ? 'Salvando…' : 'Salvar conta'}
      </button>
      {error && (
        <p className="acc-error" role="alert">
          {error}
        </p>
      )}
    </form>
  );
}
