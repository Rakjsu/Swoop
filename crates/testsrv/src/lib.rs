//! Servidor HTTP de teste do Swoop (só para testes e ensaios manuais).
//!
//! Serve arquivos determinísticos em `/file/{nome}` com Range, ETag e
//! If-Range, mais botões de falha (ver `knobs`). `/stats` expõe picos de
//! conexões e downloads; `POST /admin/generation/{n}` "troca" todos os
//! arquivos (novo conteúdo e novo ETag), para testar arquivo mudado no servidor.

pub mod content;
mod file;
pub mod knobs;
pub mod stats;

pub use content::{effective_seed, fill, sha256_hex};
pub use stats::Stats;

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use file::Shared;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::oneshot;

/// Servidor rodando em segundo plano; para quando é descartado.
pub struct TestServer {
    addr: SocketAddr,
    shared: Shared,
    shutdown: Option<oneshot::Sender<()>>,
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
        };
        let app = router(shared.clone());
        let listener = tokio::net::TcpListener::bind(addr).await?;
        let addr = listener.local_addr()?;
        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await;
        });
        Ok(Self {
            addr,
            shared,
            shutdown: Some(tx),
        })
    }

    /// Endereço real (porta escolhida pelo SO).
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// URL de um arquivo com os botões em `query` (ex.: `"size=1048576&rate=65536"`).
    pub fn file_url(&self, name: &str, query: &str) -> String {
        format!("http://{}/file/{name}?{query}", self.addr)
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
            let _ = tx.send(());
        }
    }
}

/// Rotas do servidor.
fn router(shared: Shared) -> Router {
    Router::new()
        .route("/file/{name}", get(file::serve))
        .route("/stats", get(stats_json))
        .route("/admin/generation/{n}", post(set_generation))
        .route("/admin/reset", post(reset))
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
