//! O serviço (banco + motor) dentro do app: aberto no `setup` e encerrado
//! na saída, gravando o último checkpoint de cada download.

use std::sync::{Arc, Mutex};
use std::time::Duration;
use swoop_api::ApiError;
use swoop_core::Settings;
use swoop_service::{Service, ServiceOptions};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Manager};

/// Tempo máximo esperando o motor gravar o progresso ao sair.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// Estado do app: o serviço (ou por que não abriu) e a assinatura da UI.
pub struct EngineState {
    service: Result<Arc<Service>, String>,
    /// Tarefa que empurra retratos e avisos para a UI (uma por vez).
    pub live: Mutex<Option<JoinHandle<()>>>,
}

impl EngineState {
    /// O serviço, ou um erro que a UI mostra no lugar da lista.
    pub fn service(&self) -> Result<&Arc<Service>, ApiError> {
        self.service
            .as_ref()
            .map_err(|e| ApiError::Unavailable(format!("o motor não abriu: {e}")))
    }
}

/// Abre o serviço na pasta de dados do app (`SWOOP_DATA_DIR` tem
/// prioridade; em dev o identificador `.dev` separa os dados do app
/// instalado). Uma falha não derruba o app: vira tela de erro.
pub fn open(app: &AppHandle) -> EngineState {
    let data_dir =
        swoop_service::data_dir_from_env().or_else(|| app.path().app_local_data_dir().ok());
    let opts = ServiceOptions {
        data_dir,
        settings: Settings::default(),
    };
    let service = tauri::async_runtime::block_on(Service::open(opts))
        .map(Arc::new)
        .map_err(|e| {
            tracing::error!("o motor não abriu: {e}");
            e.to_string()
        });
    if let Ok(s) = &service {
        tracing::info!("dados em {}", s.data_dir().display());
    }
    EngineState {
        service,
        live: Mutex::new(None),
    }
}

/// Para o motor esperando o checkpoint (no máximo `SHUTDOWN_TIMEOUT`). Um
/// corte no meio não corrompe nada: o banco só afirma bytes já no disco.
pub fn shutdown(app: &AppHandle) {
    let Some(state) = app.try_state::<EngineState>() else {
        return;
    };
    let Ok(service) = state.service.clone() else {
        return;
    };
    if let Some(live) = state.live.lock().expect("mutex").take() {
        live.abort();
    }
    let done =
        tauri::async_runtime::block_on(tokio::time::timeout(SHUTDOWN_TIMEOUT, service.shutdown()));
    match done {
        Ok(()) => tracing::info!("progresso gravado; saindo"),
        Err(_) => tracing::warn!("o motor demorou para parar; saindo assim mesmo"),
    }
}
