import { useCallback, useState } from 'react';

const KEY = 'swoop.collapsed';

/** Lê os pacotes recolhidos guardados (vazio se o navegador não deixar). */
function load(): Set<number> {
  try {
    const raw = window.localStorage.getItem(KEY);
    const ids: unknown = raw ? JSON.parse(raw) : [];
    return new Set(Array.isArray(ids) ? ids.filter((id): id is number => typeof id === 'number') : []);
  } catch {
    return new Set();
  }
}

/** Guarda sem reclamar: se não der, só não lembra na próxima vez. */
function save(ids: Set<number>): void {
  try {
    window.localStorage.setItem(KEY, JSON.stringify([...ids]));
  } catch {
    // armazenamento bloqueado (janela privada, cota): segue só na memória
  }
}

/** Pacotes recolhidos na lista de Downloads (lembrados neste navegador/app). */
export function useCollapsed(): [Set<number>, (id: number) => void] {
  const [collapsed, setCollapsed] = useState<Set<number>>(load);
  const toggle = useCallback((id: number) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (!next.delete(id)) next.add(id);
      save(next);
      return next;
    });
  }, []);
  return [collapsed, toggle];
}
