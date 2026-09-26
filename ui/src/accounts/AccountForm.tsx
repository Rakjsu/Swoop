import { useState } from 'react';
import type { AccountKind } from '../gen/AccountKind';
import { errorMessage, type Transport } from '../transport';

interface Props {
  transport: Transport;
  hosts: string[];
}

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
  const chosen = host || hosts[0] || '';

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    const value = secret;
    setSecret('');
    setBusy(true);
    setError(null);
    try {
      await transport.exec({ type: 'add_account', host: chosen, kind, username, secret: value });
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
            <option key={h} value={h}>
              {h}
            </option>
          ))}
        </select>
      </label>
      <label className="acc-field">
        <span>Tipo</span>
        <select value={kind} onChange={(e) => setKind(e.target.value as AccountKind)}>
          <option value="login">Usuário e senha</option>
          <option value="api_key">Chave da API</option>
        </select>
      </label>
      {kind === 'login' && (
        <label className="acc-field">
          <span>Usuário</span>
          <input type="text" autoComplete="off" value={username} onChange={(e) => setUsername(e.target.value)} />
        </label>
      )}
      <label className="acc-field">
        <span>{kind === 'login' ? 'Senha' : 'Chave'}</span>
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
