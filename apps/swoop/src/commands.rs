//! Comandos chamados pela UI via `invoke`. Cada um precisa constar em
//! `build.rs` (AppManifest) e em `capabilities/main.json`.

use swoop_api::AppInfo;

/// Nome e versão do app, exibidos na tela inicial.
#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo::current()
}
