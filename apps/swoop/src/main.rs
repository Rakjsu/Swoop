//! Swoop desktop: janela Tauri que hospeda a UI React.
//!
//! Na fase 0 só expõe `app_info`; o motor, a bandeja e os demais comandos
//! entram na fase 2 sobre o crate `swoop-service`.

// Sem console extra no Windows em release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use tauri::Manager;
use tracing_subscriber::EnvFilter;

/// Liga os logs; o nível vem de `RUST_LOG` (padrão `info`).
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

/// Traz a janela principal para a frente quando uma segunda instância é aberta.
fn focus_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn main() {
    init_tracing();
    tracing::info!("{} iniciando", swoop_core::version_line());

    tauri::Builder::default()
        // Instância única precisa ser o primeiro plugin registrado.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            focus_main_window(app);
        }))
        .invoke_handler(tauri::generate_handler![commands::app_info])
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o Swoop");
}
