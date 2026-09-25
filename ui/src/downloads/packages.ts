import type { DownloadView } from '../gen/DownloadView';
import type { LiveRow } from '../gen/LiveRow';
import { progressOf } from './format';

/** Resumo de um pacote para a linha de cabeçalho. */
export interface PackageSummary {
  id: number;
  name: string;
  count: number;
  completed: number;
  failed: number;
  done: number;
  total: number;
  speed: number;
  collapsed: boolean;
}

/** Item da lista: cabeçalho de pacote ou download. */
export type ListItem =
  | { kind: 'package'; key: string; pkg: PackageSummary }
  | { kind: 'download'; key: string; row: DownloadView };

/**
 * Agrupa os downloads por pacote (na ordem da fila) e achata numa lista só:
 * cabeçalho e, se o pacote não estiver recolhido, os downloads dele.
 */
export function buildItems(
  rows: DownloadView[],
  live: Map<number, LiveRow>,
  collapsed: Set<number>,
): ListItem[] {
  const groups = new Map<number, DownloadView[]>();
  for (const row of rows) {
    const group = groups.get(row.package_id);
    if (group) group.push(row);
    else groups.set(row.package_id, [row]);
  }
  const items: ListItem[] = [];
  for (const [id, group] of groups) {
    const pkg: PackageSummary = {
      id,
      name: group[0]?.package_name || 'Pacote',
      count: group.length,
      completed: 0,
      failed: 0,
      done: 0,
      total: 0,
      speed: 0,
      collapsed: collapsed.has(id),
    };
    for (const row of group) {
      const { done, total } = progressOf(row, live.get(row.id));
      pkg.done += done;
      pkg.total += total ?? 0;
      pkg.speed += live.get(row.id)?.speed_bps ?? 0;
      if (row.state === 'completed') pkg.completed += 1;
      if (row.state === 'failed') pkg.failed += 1;
    }
    items.push({ kind: 'package', key: `p${id}`, pkg });
    if (!pkg.collapsed) {
      for (const row of group) items.push({ kind: 'download', key: `d${row.id}`, row });
    }
  }
  return items;
}
