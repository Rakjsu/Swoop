import { invoke, isTauri } from '@tauri-apps/api/core';

/** Identidade do app, espelho de `swoop_api::AppInfo`. */
export interface AppInfo {
  name: string;
  version: string;
}

/**
 * Busca nome e versão no processo Rust. Fora do Tauri (navegador puro) devolve
 * `null`: o transporte HTTP do painel remoto só chega na fase 2.
 */
export async function loadAppInfo(): Promise<AppInfo | null> {
  if (!isTauri()) {
    return null;
  }
  return invoke<AppInfo>('app_info');
}
