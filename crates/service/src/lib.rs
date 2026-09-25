//! Serviço do Swoop: abre a pasta de dados (com trava de instância única),
//! o banco e o motor, e oferece as operações de alto nível usadas pelo CLI e,
//! pela trait `swoop_api::Backend` (em `backend.rs`), pelo app desktop e pelo
//! painel remoto.

mod backend;
mod notices;
mod paths;
mod prefs;

pub use paths::{data_dir_from_env, default_data_dir, default_download_dir};

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use swoop_api::Push;
use swoop_core::{DownloadId, DownloadState, Settings};
use swoop_engine::{Engine, EngineConfig};
use swoop_hosts::DirectResolver;
use swoop_store::history::HistoryRow;
use swoop_store::{DownloadRow, Store, StoreError, downloads, history, packages};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
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
    #[error("preferências inválidas: {0}")]
    BadSettings(String),
}

/// Como abrir o serviço.
#[derive(Debug, Clone, Default)]
pub struct ServiceOptions {
    /// Pasta de dados; `None` = `SWOOP_DATA_DIR` ou a pasta padrão do SO.
    pub data_dir: Option<PathBuf>,
    /// Preferências explícitas (CLI), que não são salvas; `None` = as salvas
    /// no banco (app, painel).
    pub settings: Option<Settings>,
}

/// Serviço aberto. Chame `shutdown` antes de sair para gravar o último
/// checkpoint.
pub struct Service {
    store: Store,
    engine: Engine,
    data_dir: PathBuf,
    /// Avisos para as interfaces (`Push::Changed`, `Push::Notice`).
    pushes: broadcast::Sender<Push>,
    /// Tarefa que traduz os eventos do motor em avisos.
    forward: JoinHandle<()>,
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
        let settings = prefs::load(&store, opts.settings).await?;
        let engine = Engine::new(
            store.clone(),
            Arc::new(DirectResolver),
            EngineConfig {
                settings,
                user_agent: swoop_net::DEFAULT_USER_AGENT.to_owned(),
            },
        )
        .map_err(|e| ServiceError::Http(e.to_string()))?;
        engine.start();
        let (pushes, _) = broadcast::channel(64);
        let forward = tokio::spawn(notices::forward(
            engine.events(),
            store.clone(),
            pushes.clone(),
        ));
        Ok(Self {
            store,
            engine,
            data_dir,
            pushes,
            forward,
            _lock: lock,
        })
    }

    /// Adiciona links num pacote novo. Com `dest`, tudo vai para lá; sem, a
    /// pasta é automática por tipo (vídeos, músicas, downloads).
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
        let auto = dest.is_none();
        let dest = dest
            .map(Path::to_path_buf)
            .unwrap_or_else(|| default_download_dir(&self.engine.settings()));
        let name = package_name.to_owned();
        let ids = self
            .store
            .call(move |c| {
                let pkg = packages::insert(c, &name, &dest, auto)?;
                urls.iter().map(|u| downloads::insert(c, pkg, u)).collect()
            })
            .await?;
        self.engine.wake();
        let _ = self.pushes.send(Push::Changed);
        Ok(ids)
    }

    /// Todos os downloads na ordem da fila.
    pub async fn rows(&self) -> Result<Vec<DownloadRow>, ServiceError> {
        Ok(self.store.call(|c| downloads::list(c)).await?)
    }

    /// Um download pelo id (`None` se já saiu da fila).
    pub async fn row(&self, id: DownloadId) -> Result<Option<DownloadRow>, ServiceError> {
        Ok(self.store.call(move |c| downloads::get(c, id)).await?)
    }

    /// Downloads que ainda vão andar sozinhos (nem finais, nem pausados).
    pub async fn pending(&self) -> Result<usize, ServiceError> {
        Ok(pending_in(&self.rows().await?))
    }

    /// As `limit` entradas mais recentes do histórico.
    pub async fn history(&self, limit: u32) -> Result<Vec<HistoryRow>, ServiceError> {
        Ok(self.store.call(move |c| history::list(c, limit)).await?)
    }

    /// Preferências em uso.
    pub fn settings(&self) -> Settings {
        self.engine.settings()
    }

    /// Valida, salva e aplica as preferências.
    pub async fn save_settings(&self, settings: Settings) -> Result<(), ServiceError> {
        let settings = prefs::save(&self.store, settings).await?;
        self.engine.update_settings(settings);
        let _ = self.pushes.send(Push::Changed);
        Ok(())
    }

    /// Troca o limite global na hora e salva.
    pub async fn set_speed_limit(&self, bps: Option<u64>) -> Result<(), ServiceError> {
        let bps = bps.filter(|b| *b > 0);
        self.engine.set_speed_limit(bps);
        prefs::save(&self.store, self.engine.settings()).await?;
        Ok(())
    }

    /// O motor (pausar, retomar, limite, progresso, eventos).
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Pasta de dados em uso.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Para o motor esperando o último checkpoint de cada download. Pode ser
    /// chamado mais de uma vez; depois dele o serviço não baixa mais nada.
    pub async fn shutdown(&self) {
        self.engine.shutdown().await;
        self.forward.abort();
    }
}

/// Quantos ainda vão andar sozinhos (nem finais, nem pausados).
fn pending_in(rows: &[DownloadRow]) -> usize {
    rows.iter()
        .filter(|r| !r.state.is_final() && r.state != DownloadState::Paused)
        .count()
}
