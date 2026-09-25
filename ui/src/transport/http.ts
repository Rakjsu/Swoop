import type { Push } from '../gen/Push';
import type { Transport } from './index';

const TOKEN_KEY = 'swoop.token';
/** Espera entre reconexões do WebSocket (cresce até o máximo). */
const RETRY_MIN_MS = 1000;
const RETRY_MAX_MS = 10000;

/**
 * Token do painel: vem no `#t=` do endereço impresso pelo `serve`. Sai da
 * barra de endereço na hora e fica só nesta aba (sessionStorage).
 */
function readToken(): string | null {
  const match = /[#&]t=([0-9a-f]{64})/.exec(window.location.hash);
  try {
    if (match?.[1]) {
      sessionStorage.setItem(TOKEN_KEY, match[1]);
      window.history.replaceState(null, '', window.location.pathname + window.location.search);
      return match[1];
    }
    return sessionStorage.getItem(TOKEN_KEY);
  } catch {
    return match?.[1] ?? null;
  }
}

/** Transporte HTTP + WebSocket; `null` sem token (página aberta sem o `#t=`). */
export function createHttpTransport(): Transport | null {
  const token = readToken();
  if (!token) return null;

  async function call<T>(path: string, init?: RequestInit): Promise<T> {
    const res = await fetch(`/api/v1${path}`, {
      ...init,
      headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
    });
    if (res.status === 401) {
      throw { message: 'Sessão inválida: abra o endereço novo que o swoop-cli serve imprimiu.' };
    }
    const body: unknown = await res.json().catch(() => null);
    if (!res.ok) throw body ?? { message: `falha HTTP ${res.status}` };
    return body as T;
  }

  const listeners = new Set<(push: Push) => void>();
  let socket: WebSocket | null = null;
  let retry = RETRY_MIN_MS;

  /** Um WebSocket por página; ao cair, reconecta e pede para recarregar a lista. */
  function connect(): void {
    const proto = window.location.protocol === 'https:' ? 'wss' : 'ws';
    const ws = new WebSocket(`${proto}://${window.location.host}/api/v1/ws`);
    socket = ws;
    ws.onopen = () => {
      ws.send(token as string);
      retry = RETRY_MIN_MS;
      listeners.forEach((listener) => listener({ type: 'changed' }));
    };
    ws.onmessage = (ev) => {
      const push = JSON.parse(String(ev.data)) as Push;
      listeners.forEach((listener) => listener(push));
    };
    ws.onclose = () => {
      socket = null;
      window.setTimeout(connect, retry);
      retry = Math.min(retry * 2, RETRY_MAX_MS);
    };
  }

  return {
    exec: (cmd) => call('/exec', { method: 'POST', body: JSON.stringify(cmd) }),
    list: () => call('/list'),
    history: (limit) => call(`/history?limit=${limit}`),
    settings: () => call('/settings'),
    onPush(listener) {
      listeners.add(listener);
      if (!socket) connect();
      return () => {
        listeners.delete(listener);
      };
    },
  };
}
