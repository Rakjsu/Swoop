//! Uma transferência: N conexões + a thread escritora de um download.
//! O primeiro erro de qualquer conexão cancela as outras; erro de disco da
//! escritora tem precedência (é a causa real quando as conexões param).

use crate::error::TransferError;
use crate::progress::JobProgress;
use crate::table::Table;
use crate::worker::{self, WorkerCtx};
use crate::writer::{self, WriterCfg};
use async_speed_limit::Limiter;
use std::fs::File;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use swoop_core::{DownloadId, Resolved, Resolver};
use swoop_net::reqwest;
use swoop_store::Store;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

/// Pedaços em trânsito entre conexões e escritora (contrapressão).
const CHANNEL_CAPACITY: usize = 64;

/// Tudo que uma transferência precisa.
pub struct TransferSpec {
    pub id: DownloadId,
    pub client: reqwest::Client,
    pub resolved: Arc<Resolved>,
    pub resolver: Arc<dyn Resolver>,
    pub validator: Option<String>,
    pub ranged: bool,
    pub file: File,
    pub table: Table,
    pub workers: usize,
    pub store: Store,
    pub limiter: Limiter,
    pub cancel: CancellationToken,
    pub max_retries: u32,
    pub progress: Arc<JobProgress>,
}

/// Roda até completar, falhar ou ser cancelada. Devolve o tamanho final.
pub async fn run(spec: TransferSpec) -> Result<u64, TransferError> {
    let table = Arc::new(Mutex::new(spec.table));
    spec.progress.received.store(
        table.lock().expect("tabela envenenada").received(),
        Ordering::Relaxed,
    );

    let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);
    let writer = writer::spawn(
        WriterCfg {
            file: spec.file,
            table: table.clone(),
            store: spec.store,
            id: spec.id,
            persist: spec.ranged,
        },
        rx,
    )
    .map_err(|e| TransferError::Disk(e.to_string()))?;

    let cancel = spec.cancel.child_token();
    let ctx = Arc::new(WorkerCtx {
        client: spec.client,
        resolved: spec.resolved,
        resolver: spec.resolver,
        validator: spec.validator,
        ranged: spec.ranged,
        table: table.clone(),
        writer: tx,
        limiter: spec.limiter,
        cancel: cancel.clone(),
        max_retries: spec.max_retries,
        progress: spec.progress,
    });

    let mut set = JoinSet::new();
    for w in 0..spec.workers.max(1) {
        set.spawn(worker::run(ctx.clone(), w));
    }
    // Só as conexões seguram o canal: quando todas saírem, a escritora termina.
    drop(ctx);

    let mut first_err: Option<TransferError> = None;
    while let Some(joined) = set.join_next().await {
        let result =
            joined.unwrap_or_else(|e| Err(TransferError::Fatal(format!("conexão abortou: {e}"))));
        if let Err(e) = result
            && first_err.is_none()
        {
            if e != TransferError::Cancelled {
                tracing::debug!(id = spec.id.0, "transferência parando: {e}");
            }
            first_err = Some(e);
            cancel.cancel();
        }
    }

    let written = tokio::task::spawn_blocking(move || writer.join())
        .await
        .map_err(|e| TransferError::Fatal(format!("escritora abortou: {e}")))?
        .unwrap_or_else(|_| Err(TransferError::Fatal("a escritora entrou em pânico".into())));
    written?;
    if let Some(e) = first_err {
        return Err(e);
    }

    let t = table.lock().expect("tabela envenenada");
    if !t.is_complete() {
        return Err(TransferError::Transient(
            "o download terminou incompleto".into(),
        ));
    }
    Ok(t.total())
}
