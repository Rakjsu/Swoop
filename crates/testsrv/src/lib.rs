//! Servidor HTTP de teste do Swoop (só para testes e ensaios manuais).
//!
//! Serve arquivos determinísticos em `/file/{nome}` com Range, ETag e
//! If-Range, mais botões de falha (ver `knobs`). `/stats` expõe picos de
//! conexões e downloads; `POST /admin/generation/{n}` "troca" todos os
//! arquivos (novo conteúdo e novo ETag), para testar arquivo mudado no servidor.
//! `/{code}` é um XFileSharing falso (download grátis com contador e captcha).
//! Num segundo endereço (`rapidgator_*`), um Rapidgator falso (`rapidgator`).

pub mod content;
mod file;
pub mod knobs;
pub mod rapidgator;
pub mod rapidgator_api;
mod rapidgator_captcha;
pub mod rapidgator_folder;
mod rapidgator_pages;
pub mod stats;
pub mod xfs;
pub mod xfs_premium;

pub use content::{effective_seed, fill, md5_hex, sha256_hex};
pub use rapidgator::RgStats;
pub use stats::Stats;
pub use xfs::XfsStats;

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use file::Shared;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::watch;

/// Servidor rodando em segundo plano; para quando é descartado.
pub struct TestServer {
    addr: SocketAddr,
    /// Endereço do Rapidgator falso (rotas próprias: `/file/{id}` colide).
    rg_addr: SocketAddr,
    shared: Shared,
    shutdown: Option<watch::Sender<bool>>,
}

impl TestServer {
    /// Sobe em `127.0.0.1` numa porta livre.
    pub async fn start() -> std::io::Result<Self> {
        Self::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await
    }

    /// Sobe no endereço dado.
    pub async fn bind(addr: SocketAddr) -> std::io::Result<Self> {
        let shared = Shared {
            counters: stats::Counters::default(),
            generation: Arc::new(AtomicU64::new(0)),
            requests: Arc::new(AtomicU64::new(0)),
            xfs: xfs::XfsState::default(),
            rg: rapidgator::RgState::default(),
        };
        let listener = tokio::net::TcpListener::bind(addr).await?;
        let addr = listener.local_addr()?;
        let rg_listener = tokio::net::TcpListener::bind(SocketAddr::new(addr.ip(), 0)).await?;
        let rg_addr = rg_listener.local_addr()?;
        let (tx, rx) = watch::channel(false);
        let (main, rg) = (router(shared.clone()), rg_router(shared.clone()));
        for (listener, app) in [(listener, main), (rg_listener, rg)] {
            let mut stop = rx.clone();
            tokio::spawn(async move {
                let _ = axum::serve(listener, app)
                    .with_graceful_shutdown(async move {
                        let _ = stop.changed().await;
                    })
                    .await;
            });
        }
        Ok(Self {
            addr,
            rg_addr,
            shared,
            shutdown: Some(tx),
        })
    }

    /// Origem do Rapidgator falso (`http://127.0.0.1:porta`).
    pub fn rapidgator_origin(&self) -> String {
        format!("http://{}", self.rg_addr)
    }

    /// Página de um arquivo no Rapidgator falso (`id` com 32 hex, botões em
    /// `query`).
    pub fn rapidgator_url(&self, id: &str, name: &str, query: &str) -> String {
        format!("http://{}/file/{id}/{name}.html?{query}", self.rg_addr)
    }

    /// O que o Rapidgator falso viu.
    pub fn rapidgator_stats(&self) -> RgStats {
        self.shared.rg.stats()
    }

    /// Estado do Rapidgator falso (arquivo premium, sessões, tráfego).
    pub fn rapidgator(&self) -> &rapidgator::RgState {
        &self.shared.rg
    }

    /// Endereço real (porta escolhida pelo SO).
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// URL de um arquivo com os botões em `query` (ex.: `"size=1048576&rate=65536"`).
    pub fn file_url(&self, name: &str, query: &str) -> String {
        format!("http://{}/file/{name}?{query}", self.addr)
    }

    /// Página de um arquivo no XFileSharing falso (`code` com 12 letras ou
    /// números, botões em `query`).
    pub fn xfs_url(&self, code: &str, query: &str) -> String {
        format!("http://{}/{code}?{query}", self.addr)
    }

    /// O que o XFileSharing falso viu.
    pub fn xfs_stats(&self) -> XfsStats {
        self.shared.xfs.stats()
    }

    /// Botões do arquivo que a conta premium do XFS falso recebe.
    pub fn set_xfs_premium_file(&self, query: &str) {
        self.shared.xfs.set_premium_file(query);
    }

    /// Contadores atuais.
    pub fn stats(&self) -> Stats {
        self.shared.counters.snapshot()
    }

    /// Zera picos e totais.
    pub fn reset_stats(&self) {
        self.shared.counters.reset();
    }

    /// Troca a geração de todos os arquivos (conteúdo e ETag mudam).
    pub fn set_generation(&self, generation: u64) {
        self.shared.generation.store(generation, Ordering::SeqCst);
    }

    /// Geração atual.
    pub fn generation(&self) -> u64 {
        self.shared.generation.load(Ordering::SeqCst)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(true);
        }
    }
}

/// Rotas do servidor.
fn router(shared: Shared) -> Router {
    Router::new()
        .route("/file/{name}", get(file::serve))
        .route("/{code}", get(xfs::page).post(xfs::form))
        .route("/", get(xfs_premium::my_account).post(xfs_premium::login))
        .route("/api/file/direct_link", get(xfs_premium::direct_link))
        .route("/api/account/info", get(xfs_premium::account_info))
        .route("/stats", get(stats_json))
        .route("/admin/generation/{n}", post(set_generation))
        .route("/admin/reset", post(reset))
        .with_state(shared)
}

/// Rotas do Rapidgator falso.
fn rg_router(shared: Shared) -> Router {
    Router::new()
        .route("/file/{id}", get(rapidgator::file_page))
        .route("/file/{id}/{name}", get(rapidgator::file_page))
        .route("/download/AjaxStartTimer", get(rapidgator::start_timer))
        .route("/download/AjaxGetDownloadLink", get(rapidgator::get_link))
        .route(
            "/download/captcha",
            get(rapidgator_captcha::captcha_page).post(rapidgator_captcha::captcha_submit),
        )
        .route("/dl/{token}/{name}", get(rapidgator_captcha::download))
        .route("/api/v2/user/login", post(rapidgator_api::login))
        .route("/api/v2/user/info", get(rapidgator_api::user_info))
        .route("/api/v2/file/info", get(rapidgator_api::file_info))
        .route("/api/v2/file/download", get(rapidgator_api::file_download))
        .route(
            "/premium/{id}/{name}",
            get(rapidgator_api::premium_download),
        )
        .route("/folder/{n}/{name}", get(rapidgator_folder::folder))
        .with_state(shared)
}

async fn stats_json(State(st): State<Shared>) -> axum::Json<Stats> {
    axum::Json(st.counters.snapshot())
}

async fn set_generation(State(st): State<Shared>, Path(n): Path<u64>) -> &'static str {
    st.generation.store(n, Ordering::SeqCst);
    "ok"
}

async fn reset(State(st): State<Shared>) -> &'static str {
    st.counters.reset();
    "ok"
}
