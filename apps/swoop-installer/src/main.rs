//! Instalador personalizado do Swoop (mesmo desenho do instalador do NeoStream).
//!
//! Uma janela sem borda com a identidade do app conduz a instalação; por
//! baixo, o setup NSIS padrão (embutido neste executável) roda em silêncio.
//! Assim a atualização automática continua usando o mesmo NSIS.

// Sem console extra no Windows em release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod install;

use serde::Serialize;
use std::path::PathBuf;
use tauri_plugin_dialog::DialogExt;

/// Setup NSIS embutido pelo `build.rs` (vazio em builds de desenvolvimento).
static PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.exe"));

/// Versão do Swoop que este instalador carrega.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Dados da tela de boas-vindas.
#[derive(Serialize)]
struct InstallerInfo {
    version: String,
    default_dir: String,
    has_payload: bool,
}

#[tauri::command]
fn installer_info() -> InstallerInfo {
    InstallerInfo {
        version: VERSION.to_owned(),
        default_dir: install::default_dir().display().to_string(),
        has_payload: !PAYLOAD.is_empty(),
    }
}

/// Abre o seletor de pasta; devolve a pasta final (com `\Swoop`).
#[tauri::command]
async fn choose_dir(app: tauri::AppHandle, current: String) -> Option<String> {
    let start = PathBuf::from(&current);
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = app
            .dialog()
            .file()
            .set_title("Escolher pasta de instalação");
        if let Some(parent) = start.parent().filter(|p| p.exists()) {
            dialog = dialog.set_directory(parent);
        }
        dialog.blocking_pick_folder()
    })
    .await
    .ok()??;
    let dir = picked.into_path().ok()?;
    Some(install::normalize_dir(&dir).display().to_string())
}

/// Roda o setup em silêncio e devolve o código de saída (0 = sucesso).
#[tauri::command]
async fn start_install(dir: String) -> Result<i32, String> {
    tauri::async_runtime::spawn_blocking(move || {
        install::run(PAYLOAD, VERSION, &PathBuf::from(dir))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Abre o Swoop recém-instalado sem herdar o administrador.
#[tauri::command]
fn launch_app(dir: String) -> Result<(), String> {
    install::launch(&PathBuf::from(dir))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            installer_info,
            choose_dir,
            start_install,
            launch_app
        ])
        .run(tauri::generate_context!())
        .expect("falha ao abrir o instalador do Swoop");
}
