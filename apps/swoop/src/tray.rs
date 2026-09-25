//! Ícone da bandeja: reabre a janela, pausa/retoma tudo e encerra o app.
//! Com ele, o X da janela só esconde (os downloads continuam).

use crate::engine::EngineState;
use swoop_api::{Backend, Command};
use tauri::menu::{MenuBuilder, MenuEvent};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

/// Se a bandeja foi criada (sem ela, fechar a janela encerra o app).
pub struct TrayState {
    pub available: bool,
}

/// Cria o ícone com o menu.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let menu = MenuBuilder::new(app)
        .text("open", "Abrir o Swoop")
        .separator()
        .text("pause_all", "Pausar tudo")
        .text("resume_all", "Retomar tudo")
        .separator()
        .text("quit", "Sair")
        .build()?;
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Swoop")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Mostra e traz para a frente a janela principal.
pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Itens do menu. Roda na thread da interface: nada de esperar aqui.
fn on_menu(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        "open" => show_main_window(app),
        "pause_all" => run(app, Command::PauseAll),
        "resume_all" => run(app, Command::ResumeAll),
        // Passa por `RunEvent::Exit`, que grava o progresso.
        "quit" => app.exit(0),
        _ => {}
    }
}

/// Executa um pedido em segundo plano; erro só vai para o log.
fn run(app: &AppHandle, cmd: Command) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<EngineState>();
        let result = match state.service() {
            Ok(service) => service.exec(cmd).await.map(|_| ()),
            Err(e) => Err(e),
        };
        if let Err(e) = result {
            tracing::warn!("bandeja: {e}");
        }
    });
}
