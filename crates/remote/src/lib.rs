//! Painel do Swoop no navegador, servido só em 127.0.0.1.
//!
//! - `/` e demais caminhos: arquivos da UI (a mesma do app desktop);
//! - `/api/v1/{exec,list,history,settings}`: JSON, com `Authorization: Bearer`;
//! - `/api/v1/ws`: retratos e avisos; a 1ª mensagem do cliente é o token.
//!
//! Proteções (`guard`): token aleatório por execução comparado em tempo
//! constante, `Host` e `Origin` só de 127.0.0.1/localhost na porta certa
//! (contra DNS rebinding e sites abertos no mesmo navegador), CSP estrita.
//! A abertura para a rede local (celular) é da fase 8.

mod guard;
mod handlers;
mod ws;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::{get, post};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use swoop_api::Backend;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tower_http::services::{ServeDir, ServeFile};

pub use guard::Token;

/// Como subir o painel.
#[derive(Debug, Clone, Default)]
pub struct RemoteOptions {
    /// Porta em 127.0.0.1 (0 = qualquer livre).
    pub port: u16,
    /// Pasta com o build da UI (`ui/dist`); `None` = só a API.
    pub ui_dir: Option<PathBuf>,
}

/// Erro ao subir o painel.
#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error("não consegui abrir a porta {0}: {1}")]
    Bind(u16, std::io::Error),
    #[error("falha ao gerar o token: {0}")]
    Token(String),
}

/// Estado compartilhado pelas rotas.
pub(crate) struct Shared {
    pub backend: Arc<dyn Backend>,
    pub token: Token,
    pub port: u16,
}

/// Painel rodando. `stop` encerra; soltar sem `stop` deixa rodando.
pub struct Running {
    pub addr: SocketAddr,
    pub token: Token,
    stop: CancellationToken,
    handle: JoinHandle<()>,
}

impl Running {
    /// Endereço para abrir no navegador (o token vai no `#`, que o navegador
    /// não manda ao servidor nem a outros sites).
    pub fn url(&self) -> String {
        format!("http://{}/#t={}", self.addr, self.token.as_str())
    }

    /// Para de aceitar conexões e espera o servidor terminar.
    pub async fn stop(self) {
        self.stop.cancel();
        let _ = self.handle.await;
    }
}

/// Sobe o painel em 127.0.0.1.
pub async fn start(backend: Arc<dyn Backend>, opts: RemoteOptions) -> Result<Running, RemoteError> {
    let token = Token::generate().map_err(|e| RemoteError::Token(e.to_string()))?;
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, opts.port))
        .await
        .map_err(|e| RemoteError::Bind(opts.port, e))?;
    let addr = listener
        .local_addr()
        .map_err(|e| RemoteError::Bind(opts.port, e))?;
    let shared = Arc::new(Shared {
        backend,
        token: token.clone(),
        port: addr.port(),
    });
    let app = router(shared, opts.ui_dir);
    let stop = CancellationToken::new();
    let signal = stop.clone();
    let handle = tokio::spawn(async move {
        let serve = axum::serve(listener, app).with_graceful_shutdown(signal.cancelled_owned());
        if let Err(e) = serve.await {
            tracing::error!("painel parou com erro: {e}");
        }
    });
    tracing::info!("painel em http://{addr}/");
    Ok(Running {
        addr,
        token,
        stop,
        handle,
    })
}

/// Rotas: API sob `/api/v1`, o resto são arquivos da UI.
fn router(shared: Arc<Shared>, ui_dir: Option<PathBuf>) -> Router {
    let api = Router::new()
        .route("/exec", post(handlers::exec))
        .route("/list", get(handlers::list))
        .route("/history", get(handlers::history))
        .route("/collector", get(handlers::collector))
        .route("/settings", get(handlers::settings))
        .route_layer(from_fn_with_state(shared.clone(), guard::bearer))
        .route("/ws", get(ws::upgrade));
    let mut app = Router::new().nest("/api/v1", api);
    if let Some(dir) = ui_dir {
        let index = ServeFile::new(dir.join("index.html"));
        app = app.fallback_service(ServeDir::new(dir).fallback(index));
    }
    app.layer(from_fn_with_state(shared.clone(), guard::origin))
        .with_state(shared)
}
