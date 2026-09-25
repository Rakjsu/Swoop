import { invoke, isTauri } from '@tauri-apps/api/core';
import type { AppInfo } from './gen/AppInfo';

export type { AppInfo };

/**
 * Busca nome e versão no processo Rust. Fora do Tauri (navegador puro) devolve
 * `null`: o painel no navegador chega na v0.3.0.
 */
export async function loadAppInfo(): Promise<AppInfo | null> {
  if (!isTauri()) {
    return null;
  }
  return invoke<AppInfo>('app_info');
}
