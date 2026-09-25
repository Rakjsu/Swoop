//! Serviço do Swoop: abre a pasta de dados (com trava de instância única),
//! o banco e o motor, e oferece as operações de alto nível usadas pelo CLI e,
//! a partir da fase 2, pelo app desktop e pelo painel remoto.

mod paths;

pub use paths::{default_data_dir, default_download_dir};

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use swoop_core::{DownloadId, DownloadState, Settings};
use swoop_engine::{Engine, EngineConfig};
use swoop_hosts::DirectResolver;
use swoop_store::{DownloadRow, Store, StoreError, downloads, packages};
use url::Url;

/// Nome do arquivo do banco dentro da pasta de dados.
const DB_FILE: &str = "swoop.sqlite";
/// Trava de instância única dentro da pasta de dados.
const LOCK_FILE: &str = "swoop.lock";

/// Erro ao abrir ou operar o serviço (mensagens para o usuário).
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("outro Swoop já está usando a pasta de dados {0}")]
    Busy(PathBuf),
    #[error("falha de disco: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("falha ao preparar a rede: {0}")]
    Http(String),
    #[error("link inválido: {0}")]
    BadLink(String),
}

/// Como abrir o serviço.
#[derive(Debug, Clone, Default)]
pub struct ServiceOptions {
    /// Pasta de dados; `None` = `SWOOP_DATA_DIR` ou a pasta padrão do SO.
    pub data_dir: Option<PathBuf>,
    pub settings: Settings,
}

/// Serviço aberto. Solte com `shutdown` para gravar o último checkpoint.
pub struct Service {
    store: Store,
    engine: Engine,
    data_dir: PathBuf,
    settings: Settings,
    _lock: File,
}

impl Service {
    /// Trava a pasta de dados, abre o banco, recupera downloads interrompidos
    /// e liga o motor.
    pub async fn open(opts: ServiceOptions) -> Result<Self, ServiceError> {
        swoop_net::install_crypto_provider();
        let data_dir = opts.data_dir.unwrap_or_else(default_data_dir);
        std::fs::create_dir_all(&data_dir)?;
        let lock = File::create(data_dir.join(LOCK_FILE))?;
        if lock.try_lock().is_err() {
            return Err(ServiceError::Busy(data_dir));
        }

        let store = Store::open(&data_dir.join(DB_FILE))?;
        let recovered = store.call(|c| downloads::recover_all(c)).await?;
        if recovered > 0 {
            tracing::info!("{recovered} download(s) interrompido(s) voltaram para a fila");
        }
        let engine = Engine::new(
            store.clone(),
            Arc::new(DirectResolver),
            EngineConfig {
                settings: opts.settings.clone(),
                user_agent: swoop_net::DEFAULT_USER_AGENT.to_owned(),
            },
        )
        .map_err(|e| ServiceError::Http(e.to_string()))?;
        engine.start();
        Ok(Self {
            store,
            engine,
            data_dir,
            settings: opts.settings,
            _lock: lock,
        })
    }

    /// Adiciona links num pacote novo com destino `dest` (padrão: pasta de
    /// downloads das preferências).
    pub async fn add_links(
        &self,
        links: &[String],
        dest: Option<&Path>,
        package_name: &str,
    ) -> Result<Vec<DownloadId>, ServiceError> {
        let urls: Vec<String> = links
            .iter()
            .map(|l| {
                Url::parse(l.trim())
                    .map(|u| u.to_string())
                    .map_err(|_| ServiceError::BadLink(l.clone()))
            })
            .collect::<Result<_, _>>()?;
        let dest = dest
            .map(Path::to_path_buf)
            .unwrap_or_else(|| default_download_dir(&self.settings));
        let name = package_name.to_owned();
        let ids = self
            .store
            .call(move |c| {
                let pkg = packages::insert(c, &name, &dest)?;
                urls.iter().map(|u| downloads::insert(c, pkg, u)).collect()
            })
            .await?;
        self.engine.wake();
        Ok(ids)
    }

    /// Todos os downloads na ordem da fila.
    pub async fn list(&self) -> Result<Vec<DownloadRow>, ServiceError> {
        Ok(self.store.call(|c| downloads::list(c)).await?)
    }

    /// Downloads que ainda vão andar sozinhos (nem finais, nem pausados).
    pub async fn pending(&self) -> Result<usize, ServiceError> {
        let rows = self.list().await?;
        Ok(rows
            .iter()
            .filter(|r| !r.state.is_final() && r.state != DownloadState::Paused)
            .count())
    }

    /// O motor (pausar, retomar, limite, progresso, eventos).
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Pasta de dados em uso.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Para o motor esperando o último checkpoint de cada download.
    pub async fn shutdown(self) {
        self.engine.shutdown().await;
    }
}
