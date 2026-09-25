//! Ciclo de vida de um download depois que o agendador o escolhe:
//! resolver → sondar → preparar arquivo → transferir → conferir → renomear.
//!
//! O estado no banco só muda por `downloads::transition`; se outro ator (o
//! usuário pausando, removendo) mudar o estado no meio, a próxima transição
//! do job falha e ele sai quieto (`TransferError::Cancelled`).

mod prepare;
mod settle;

use crate::error::TransferError;
use crate::progress::JobProgress;
use crate::{probe, transfer};
use async_speed_limit::Limiter;
use std::sync::Arc;
use swoop_core::{
    DownloadId, Event, HostError, HttpFailure, ResolveRequest, Resolved, Resolver, Settings,
};
use swoop_net::reqwest;
use swoop_store::{Store, downloads};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use url::Url;

pub use settle::EngineEvent;

/// Re-resoluções seguidas (link expirando, arquivo mudando) antes de desistir.
const MAX_RERESOLVES: u32 = 3;

/// Dependências de um job.
#[derive(Clone)]
pub struct JobCtx {
    pub store: Store,
    pub resolver: Arc<dyn Resolver>,
    pub client: reqwest::Client,
    pub limiter: Limiter,
    pub settings: Settings,
    /// Conexões liberadas pelo agendador (limite por servidor).
    pub granted_conns: u16,
    pub cancel: CancellationToken,
    pub progress: Arc<JobProgress>,
    pub events: broadcast::Sender<EngineEvent>,
}

/// Executa o download `id` (já em `resolving`) e grava o desfecho.
pub async fn run(ctx: JobCtx, id: DownloadId) {
    let result = drive(&ctx, id).await;
    settle::settle(&ctx, id, result).await;
}

/// Laço resolver → transferir, repetindo quando o link expira.
async fn drive(ctx: &JobCtx, id: DownloadId) -> Result<u64, TransferError> {
    let row = ctx
        .store
        .call(move |c| downloads::get(c, id))
        .await?
        .ok_or(TransferError::Cancelled)?;
    let url = Url::parse(&row.url).map_err(|_| TransferError::Fatal("link inválido".into()))?;

    let mut attempt = 0;
    loop {
        let resolved = Arc::new(resolve(ctx, &url, attempt).await?);
        let probe = match probe::probe(&ctx.client, &resolved).await {
            Ok(p) => p,
            Err(f) => match from_failure(ctx.resolver.as_ref(), &resolved.host_key, &f) {
                // Link recém-resolvido já negado: resolver de novo (ainda em `resolving`).
                TransferError::Reresolve if attempt < MAX_RERESOLVES => {
                    attempt += 1;
                    continue;
                }
                e => return Err(e),
            },
        };
        let prepared = prepare::prepare(ctx, id, &resolved, &probe).await?;
        transition(ctx, id, Event::Resolved).await?;

        let spec = transfer::TransferSpec {
            id,
            client: ctx.client.clone(),
            resolved: resolved.clone(),
            resolver: ctx.resolver.clone(),
            validator: probe.validator(),
            ranged: prepared.ranged,
            file: prepared.file,
            table: prepared.table,
            workers: prepared.workers,
            store: ctx.store.clone(),
            limiter: ctx.limiter.clone(),
            cancel: ctx.cancel.clone(),
            max_retries: ctx.settings.max_retries,
            progress: ctx.progress.clone(),
        };
        match transfer::run(spec).await {
            Ok(size) => return settle::finish(ctx, id, &resolved, &prepared.paths, size).await,
            Err(e @ (TransferError::Reresolve | TransferError::FileChanged))
                if attempt < MAX_RERESOLVES =>
            {
                attempt += 1;
                tracing::info!(id = id.0, attempt, "{e}");
                if e == TransferError::FileChanged {
                    ctx.store
                        .call(move |c| swoop_store::segments::clear(c, id))
                        .await?;
                }
                transition(ctx, id, Event::Reresolve).await?;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Pede o link direto ao resolvedor e converte o erro.
async fn resolve(ctx: &JobCtx, url: &Url, attempt: u32) -> Result<Resolved, TransferError> {
    let req = ResolveRequest {
        url: url.clone(),
        attempt,
    };
    let host = url.host_str().unwrap_or_default().to_owned();
    let resolved = tokio::select! {
        _ = ctx.cancel.cancelled() => return Err(TransferError::Cancelled),
        r = ctx.resolver.resolve(&req) => r,
    };
    resolved.map_err(|e| match e {
        HostError::Offline => TransferError::Fatal("arquivo offline ou removido".into()),
        HostError::Wait { until, reason } => TransferError::Wait {
            until,
            reason,
            message: reason.describe().into(),
        },
        HostError::Changed(m) => TransferError::Fatal(format!(
            "o servidor mudou e o plugin precisa de atualização: {m}"
        )),
        HostError::Network(m) => TransferError::Transient(m),
        HostError::Http(status) => from_failure(
            ctx.resolver.as_ref(),
            &host,
            &HttpFailure {
                status: Some(status),
                ..Default::default()
            },
        ),
        HostError::Unsupported => TransferError::Fatal("link não suportado".into()),
        e @ (HostError::BrowserRequired(_) | HostError::AccessDenied) => {
            TransferError::Fatal(e.to_string())
        }
    })
}

/// Falha HTTP fora das conexões (sonda, resolução) vira parada do download.
fn from_failure(resolver: &dyn Resolver, host: &str, f: &HttpFailure) -> TransferError {
    let message = crate::request::failure_message(f);
    match resolver.classify(host, f) {
        swoop_core::ErrorClass::Retry { .. } => TransferError::Transient(message),
        swoop_core::ErrorClass::Reresolve => TransferError::Reresolve,
        swoop_core::ErrorClass::Wait { until, reason } => TransferError::Wait {
            until,
            reason,
            message,
        },
        swoop_core::ErrorClass::Fatal(m) => TransferError::Fatal(m),
    }
}

/// Aplica uma transição; recusa vira `Cancelled` (outro ator mudou o estado).
async fn transition(ctx: &JobCtx, id: DownloadId, event: Event) -> Result<(), TransferError> {
    ctx.store
        .call(move |c| downloads::transition(c, id, event))
        .await?;
    let _ = ctx.events.send(EngineEvent::Changed);
    Ok(())
}
