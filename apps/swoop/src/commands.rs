//! Comandos chamados pela UI via `invoke`. Cada um precisa constar em
//! `build.rs` (AppManifest) e em `capabilities/main.json`.
//!
//! Os argumentos chegam em camelCase do JS: `subscribe({ onPush })`.

use crate::engine::EngineState;
use std::path::PathBuf;
use swoop_api::{
    ApiError, AppInfo, Backend, CollectorView, Command, DownloadView, HistoryView, Push, Reply,
    Settings,
};
use swoop_core::DownloadId;
use tauri::State;
use tauri::ipc::Channel;

/// Nome e versão do app, exibidos no cabeçalho.
#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo::current()
}

/// Executa um pedido da UI (adicionar, pausar, remover…).
#[tauri::command]
pub async fn exec(state: State<'_, EngineState>, cmd: Command) -> Result<Reply, ApiError> {
    state.service()?.exec(cmd).await
}

/// A fila inteira, na ordem.
#[tauri::command]
pub async fn list(state: State<'_, EngineState>) -> Result<Vec<DownloadView>, ApiError> {
    state.service()?.list().await
}

/// As `limit` entradas mais recentes do histórico.
#[tauri::command]
pub async fn history(
    state: State<'_, EngineState>,
    limit: u32,
) -> Result<Vec<HistoryView>, ApiError> {
    Backend::history(state.service()?.as_ref(), limit).await
}

/// Links do coletor, na ordem em que entraram.
#[tauri::command]
pub async fn collector(state: State<'_, EngineState>) -> Result<Vec<CollectorView>, ApiError> {
    Backend::collector(state.service()?.as_ref()).await
}

/// Preferências em uso (tela de Opções).
#[tauri::command]
pub async fn settings(state: State<'_, EngineState>) -> Result<Settings, ApiError> {
    Backend::settings(state.service()?.as_ref()).await
}

/// Passa a empurrar retratos (≈5 Hz) e avisos para a UI pelo canal. Só uma
/// assinatura vale por vez: a nova (recarga da página) derruba a anterior.
#[tauri::command]
pub async fn subscribe(
    state: State<'_, EngineState>,
    on_push: Channel<Push>,
) -> Result<(), ApiError> {
    let mut sub = state.service()?.subscribe();
    let task = tauri::async_runtime::spawn(async move {
        let mut push = Some(sub.current());
        while let Some(p) = push {
            if on_push.send(p).is_err() {
                return;
            }
            push = sub.next().await;
        }
    });
    if let Some(old) = state.live.lock().expect("mutex").replace(task) {
        old.abort();
    }
    Ok(())
}

/// Mostra o arquivo na pasta (Explorer). O caminho vem do banco, nunca da UI.
#[tauri::command]
pub async fn reveal_download(state: State<'_, EngineState>, id: i64) -> Result<(), ApiError> {
    let row = state
        .service()?
        .row(DownloadId(id))
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::Invalid("o download não está mais na lista".into()))?;
    let path: PathBuf = row
        .final_path
        .or(row.part_path)
        .filter(|p| p.exists())
        .ok_or_else(|| ApiError::Invalid("o arquivo ainda não existe no disco".into()))?;
    tauri_plugin_opener::reveal_item_in_dir(&path)
        .map_err(|e| ApiError::Internal(format!("não consegui abrir a pasta: {e}")))
}
