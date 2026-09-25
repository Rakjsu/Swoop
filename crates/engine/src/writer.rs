//! Thread escritora de um download: grava os pedaços na posição certa e faz
//! o checkpoint (sync_data do arquivo e só DEPOIS o SQLite). Assim o banco
//! nunca afirma bytes que ainda não estão no disco, e matar o processo a
//! qualquer momento perde no máximo o último intervalo.

use crate::disk;
use crate::error::TransferError;
use crate::table::Table;
use bytes::Bytes;
use std::fs::File;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use swoop_core::DownloadId;
use swoop_store::{Store, segments};
use tokio::sync::mpsc;

/// Checkpoint a cada intervalo…
const CHECKPOINT_EVERY: Duration = Duration::from_millis(500);
/// …ou a cada tantos bytes, o que vier primeiro.
const CHECKPOINT_BYTES: u64 = 8 * 1024 * 1024;

/// Pedaço recebido por uma conexão.
#[derive(Debug)]
pub struct WriteJob {
    pub idx: u32,
    pub offset: u64,
    pub data: Bytes,
}

/// O que a escritora precisa.
pub struct WriterCfg {
    pub file: File,
    pub table: Arc<Mutex<Table>>,
    pub store: Store,
    pub id: DownloadId,
    /// `false` para downloads sem retomada (nada a persistir).
    pub persist: bool,
}

/// Sobe a thread; ela termina quando todos os remetentes somem ou num erro de disco.
pub fn spawn(
    cfg: WriterCfg,
    rx: mpsc::Receiver<WriteJob>,
) -> std::io::Result<JoinHandle<Result<(), TransferError>>> {
    std::thread::Builder::new()
        .name(format!("swoop-writer-{}", cfg.id.0))
        .spawn(move || run(cfg, rx))
}

/// Laço principal: grava, marca, faz checkpoint periódico e um final.
fn run(cfg: WriterCfg, mut rx: mpsc::Receiver<WriteJob>) -> Result<(), TransferError> {
    let mut last = Instant::now();
    let mut pending: u64 = 0;
    while let Some(job) = rx.blocking_recv() {
        let len = job.data.len() as u64;
        disk::write_all_at(&cfg.file, &job.data, job.offset)
            .map_err(|e| TransferError::Disk(e.to_string()))?;
        cfg.table
            .lock()
            .expect("tabela envenenada")
            .mark_written(job.idx, job.offset + len);
        pending += len;
        if pending >= CHECKPOINT_BYTES || last.elapsed() >= CHECKPOINT_EVERY {
            checkpoint(&cfg)?;
            pending = 0;
            last = Instant::now();
        }
    }
    checkpoint(&cfg)
}

/// Garante os bytes no disco e então registra o progresso no banco.
fn checkpoint(cfg: &WriterCfg) -> Result<(), TransferError> {
    cfg.file
        .sync_data()
        .map_err(|e| TransferError::Disk(e.to_string()))?;
    if !cfg.persist {
        return Ok(());
    }
    let rows = cfg.table.lock().expect("tabela envenenada").rows();
    let id = cfg.id;
    cfg.store
        .call_blocking(move |c| segments::replace(c, id, &rows))?;
    Ok(())
}
