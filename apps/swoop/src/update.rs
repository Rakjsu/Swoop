//! Atualização pelo GitHub: procura versão nova ao abrir e a cada 6 h, avisa
//! a interface (`update://available`) e, quando o usuário pede, baixa o
//! instalador, confere o sha256 e o executa em modo passivo.
//!
//! O instalador NSIS (`/P /UPDATE /R`) fecha o Swoop aberto, troca os
//! arquivos em Arquivos de Programas (pede UAC) e reabre o app como usuário
//! comum. Em dev só procura com `SWOOP_UPDATE_CHECK=1`.

use serde::Serialize;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use swoop_update::{Source, Update, Version};
use tauri::{AppHandle, Emitter, Manager, State};

/// Intervalo entre consultas ao GitHub.
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// Intervalo mínimo entre eventos de progresso do download.
const PROGRESS_EVERY: Duration = Duration::from_millis(150);

/// Atualização encontrada e se já está sendo instalada.
#[derive(Default)]
pub struct UpdateState {
    available: Mutex<Option<Update>>,
    installing: AtomicBool,
}

/// O que a interface mostra na faixa de atualização.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateView {
    pub version: String,
    pub notes: String,
}

/// Progresso do download do instalador.
#[derive(Debug, Clone, Serialize)]
struct Progress {
    received: u64,
    total: Option<u64>,
}

impl From<&Update> for UpdateView {
    fn from(u: &Update) -> Self {
        Self {
            version: u.version.to_string(),
            notes: u.notes.clone(),
        }
    }
}

/// Procurar atualizações? Sempre em release; em dev só se pedido.
fn enabled() -> bool {
    if std::env::var_os("SWOOP_NO_UPDATE").is_some() {
        return false;
    }
    !cfg!(debug_assertions) || std::env::var_os("SWOOP_UPDATE_CHECK").is_some()
}

/// Versão deste binário.
fn current_version() -> Version {
    swoop_core::VERSION
        .parse()
        .expect("CARGO_PKG_VERSION é sempre x.y.z")
}

/// Sobe a tarefa que procura atualizações periodicamente.
pub fn spawn_checker(app: AppHandle) {
    if !enabled() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let client = match swoop_net::download_client(swoop_net::DEFAULT_USER_AGENT) {
            Ok(c) => c,
            Err(e) => return tracing::warn!("atualização desligada: {e}"),
        };
        loop {
            match swoop_update::check(&client, &Source::swoop(), current_version()).await {
                Ok(Some(update)) => {
                    tracing::info!("versão {} disponível", update.version);
                    let view = UpdateView::from(&update);
                    *app.state::<UpdateState>().available.lock().expect("mutex") = Some(update);
                    let _ = app.emit("update://available", view);
                }
                Ok(None) => tracing::debug!("sem atualização"),
                Err(e) => tracing::warn!("não consegui procurar atualização: {e}"),
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

/// Atualização já encontrada (a interface pergunta ao abrir).
#[tauri::command]
pub fn update_status(state: State<'_, UpdateState>) -> Option<UpdateView> {
    state
        .available
        .lock()
        .expect("mutex")
        .as_ref()
        .map(UpdateView::from)
}

/// Baixa e executa o instalador da nova versão; o app fecha em seguida.
#[tauri::command]
pub async fn install_update(app: AppHandle, state: State<'_, UpdateState>) -> Result<(), String> {
    let update = state
        .available
        .lock()
        .expect("mutex")
        .clone()
        .ok_or("nenhuma atualização disponível")?;
    if state.installing.swap(true, Ordering::SeqCst) {
        return Err("a atualização já está em andamento".into());
    }
    let result = download_and_run(&app, &update).await;
    if result.is_err() {
        state.installing.store(false, Ordering::SeqCst);
    }
    result
}

/// Download com progresso e execução do instalador.
async fn download_and_run(app: &AppHandle, update: &Update) -> Result<(), String> {
    let client =
        swoop_net::download_client(swoop_net::DEFAULT_USER_AGENT).map_err(|e| e.to_string())?;
    let dir = std::env::temp_dir().join("swoop-update");
    let last = Mutex::new(Instant::now() - PROGRESS_EVERY);
    let setup = swoop_update::download(&client, update, &dir, |received, total| {
        let mut last = last.lock().expect("mutex");
        if last.elapsed() >= PROGRESS_EVERY || Some(received) == total {
            *last = Instant::now();
            let _ = app.emit("update://progress", Progress { received, total });
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    run_installer(&setup)?;
    tracing::info!("instalador da versão {} iniciado; saindo", update.version);
    app.exit(0);
    Ok(())
}

/// Executa o instalador pelo shell do Windows (dispara o UAC do perMachine).
#[cfg(windows)]
fn run_installer(setup: &std::path::Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let verb = wide("open".as_ref());
    let file = wide(setup.as_os_str());
    let args = wide("/P /UPDATE /R".as_ref());
    // SAFETY: ponteiros para buffers terminados em zero que vivem até o fim da chamada.
    let code = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            args.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    if code > 32 {
        Ok(())
    } else if code == 5 {
        Err("o Windows não deu permissão para instalar (UAC recusado)".into())
    } else {
        Err(format!("não consegui abrir o instalador (código {code})"))
    }
}

/// Fora do Windows ainda não há instalador automático.
#[cfg(not(windows))]
fn run_installer(_setup: &std::path::Path) -> Result<(), String> {
    Err("a atualização automática por enquanto só existe no Windows".into())
}
