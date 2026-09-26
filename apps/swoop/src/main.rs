//! Swoop desktop: janela Tauri que hospeda a UI React sobre o `swoop-service`.
//!
//! - `engine`: abre o serviço (banco + motor) e o encerra gravando o progresso;
//! - `commands`: o que a UI chama via `invoke` (contrato `swoop-api`);
//! - `tray`: ícone da bandeja; o X da janela só esconde, "Sair" encerra;
//! - `notify`: notificações do sistema (fila terminou, falhou, captcha);
//! - `captcha`: janela onde o usuário resolve o captcha de um servidor;
//! - `update`: atualização pelo GitHub.

// Sem console extra no Windows em release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod captcha;
mod commands;
mod engine;
mod logs;
mod notify;
mod tray;
mod update;

use tauri::{Manager, RunEvent, WindowEvent};
use tracing_subscriber::EnvFilter;

/// Liga os logs (console e, quando a pasta de dados abre, `logs/swoop.log`);
/// o nível vem de `RUST_LOG` (padrão `info`).
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(|| logs::Tee)
        .init();
}

fn main() {
    init_tracing();
    swoop_net::install_crypto_provider();
    tracing::info!("{} iniciando", swoop_core::version_line());

    let app = tauri::Builder::default()
        // Instância única precisa ser o primeiro plugin registrado.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(update::UpdateState::default())
        .setup(|app| {
            let state = engine::open(app.handle());
            if let Ok(service) = state.service() {
                notify::spawn(app.handle().clone(), service.clone());
            }
            app.manage(state);
            let has_tray = match tray::create(app.handle()) {
                Ok(()) => true,
                Err(e) => {
                    tracing::warn!("sem ícone na bandeja: {e}");
                    false
                }
            };
            app.manage(tray::TrayState {
                available: has_tray,
            });
            update::spawn_cleanup();
            update::spawn_checker(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Com a bandeja, fechar a principal só esconde e os downloads
            // continuam (a de captcha fecha de verdade).
            if let WindowEvent::CloseRequested { api, .. } = event
                && !captcha::is_captcha(window.label())
                && window.state::<tray::TrayState>().available
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::exec,
            commands::list,
            commands::history,
            commands::collector,
            commands::accounts,
            commands::settings,
            commands::subscribe,
            commands::reveal_download,
            commands::solve_captcha,
            update::update_status,
            update::install_update
        ])
        .build(tauri::generate_context!())
        .expect("falha ao iniciar o Swoop");

    app.run(|app, event| {
        // "Sair" da bandeja, fim da última janela e o atualizador passam aqui.
        if let RunEvent::Exit = event {
            engine::shutdown(app);
        }
    });
}
