//! Desfecho de um job: concluir (conferir, renomear, histórico) ou gravar a
//! espera/falha no banco.

use super::prepare::Paths;
use super::{JobCtx, transition};
use crate::error::TransferError;
use crate::{disk, verify};
use std::time::{Duration, SystemTime};
use swoop_core::{DownloadId, Event, Resolved, WaitReason};
use swoop_store::history::{self, Outcome};
use swoop_store::{StoreError, captcha, downloads, host_state, segments, to_ms};

/// Aviso para as interfaces quando algo muda num download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineEvent {
    Started(DownloadId),
    Completed(DownloadId),
    Waiting(DownloadId),
    Failed(DownloadId, String),
    /// Saiu sem mudar o estado (pausa ou encerramento).
    Stopped(DownloadId),
    /// O servidor pediu captcha: espera o usuário.
    CaptchaNeeded(DownloadId),
    /// A fila mudou (transição de estado, item removido, esperas vencidas):
    /// as interfaces recarregam a lista.
    Changed,
}

/// Espera base entre tentativas do download inteiro (cresce por tentativa).
const RETRY_BASE: Duration = Duration::from_secs(30);

/// Bytes completos: confere, renomeia e registra.
pub async fn finish(
    ctx: &JobCtx,
    id: DownloadId,
    resolved: &Resolved,
    paths: &Paths,
    size: u64,
) -> Result<u64, TransferError> {
    transition(ctx, id, Event::Transferred).await?;

    if let Some(integrity) = resolved.integrity.clone() {
        let part = paths.part.clone();
        let ok = tokio::task::spawn_blocking(move || verify::matches(&part, &integrity))
            .await
            .map_err(|e| TransferError::Fatal(e.to_string()))?
            .map_err(|e| TransferError::Disk(e.to_string()))?;
        if !ok {
            ctx.store.call(move |c| segments::clear(c, id)).await?;
            return Err(TransferError::Fatal(
                "o arquivo baixado não confere com o hash do servidor".into(),
            ));
        }
    }
    transition(ctx, id, Event::Verified).await?;

    let (part, dir, name) = (
        paths.part.clone(),
        paths.dest_dir.clone(),
        paths.file_name.clone(),
    );
    let target = tokio::task::spawn_blocking(move || {
        let target = disk::unique_path(&dir, &name);
        disk::finalize(&part, &target).map(|()| target)
    })
    .await
    .map_err(|e| TransferError::Fatal(e.to_string()))?
    .map_err(|e| TransferError::Disk(format!("ao renomear o arquivo: {e}")))?;

    ctx.store
        .call(move |c| {
            downloads::set_final_path(c, id, &target)?;
            downloads::set_size(c, id, size)?;
            downloads::transition(c, id, Event::Finalized)?;
            segments::clear(c, id)?;
            history::record(c, id, Outcome::Completed)
        })
        .await?;
    tracing::info!(id = id.0, "concluído: {}", paths.file_name);
    let _ = ctx.events.send(EngineEvent::Completed(id));
    Ok(size)
}

/// Grava no banco o que fazer depois de uma parada.
pub async fn settle(ctx: &JobCtx, id: DownloadId, result: Result<u64, TransferError>) {
    let err = match result {
        Ok(_) => return,
        Err(TransferError::Cancelled) => {
            let _ = ctx.events.send(EngineEvent::Stopped(id));
            return;
        }
        Err(e) => e,
    };
    let max_retries = ctx.settings.max_retries;
    let host = ctx.host.clone();
    let outcome = ctx
        .store
        .call(move |c| record(c, id, &host, err, max_retries))
        .await;
    match outcome {
        Ok(Settled::Failed(message)) => {
            tracing::warn!(id = id.0, "falhou: {message}");
            let _ = ctx.events.send(EngineEvent::Failed(id, message));
        }
        Ok(Settled::Waiting) => {
            let _ = ctx.events.send(EngineEvent::Waiting(id));
        }
        Ok(Settled::Captcha) => {
            tracing::info!(id = id.0, "captcha pendente");
            let _ = ctx.events.send(EngineEvent::CaptchaNeeded(id));
        }
        Err(StoreError::Transition(_) | StoreError::NotFound(_)) => {
            let _ = ctx.events.send(EngineEvent::Stopped(id));
        }
        Err(e) => tracing::error!(id = id.0, "não consegui gravar o desfecho: {e}"),
    }
}

/// Como a parada ficou gravada.
enum Settled {
    Waiting,
    Captcha,
    Failed(String),
}

/// Grava espera, captcha pendente ou falha definitiva. Espera de "limite do
/// servidor" vale para o servidor inteiro (`host_state`).
fn record(
    c: &mut swoop_store::Connection,
    id: DownloadId,
    host: &str,
    err: TransferError,
    max_retries: u32,
) -> Result<Settled, StoreError> {
    let (kind, message) = match err {
        TransferError::Wait {
            until,
            reason,
            message,
        } => {
            downloads::wait(c, id, to_ms(until), reason, Some(&message))?;
            if reason == WaitReason::HostLimit {
                host_state::set_wait(c, host, to_ms(until), reason)?;
            }
            return Ok(Settled::Waiting);
        }
        TransferError::Captcha(challenge) => {
            let json = serde_json::to_string(&challenge)
                .map_err(|e| StoreError::Corrupt(format!("captcha: {e}")))?;
            captcha::set_challenge(c, id, &json)?;
            downloads::transition(c, id, Event::NeedCaptcha)?;
            return Ok(Settled::Captcha);
        }
        TransferError::Transient(message) => {
            let row = downloads::get(c, id)?.ok_or(StoreError::NotFound(id))?;
            let attempts = row.attempts + 1;
            downloads::set_attempts(c, id, attempts)?;
            if attempts <= max_retries {
                let until = SystemTime::now() + RETRY_BASE * attempts;
                let note = format!("{message} — nova tentativa {attempts}/{max_retries}");
                downloads::wait(c, id, to_ms(until), WaitReason::Backoff, Some(&note))?;
                return Ok(Settled::Waiting);
            }
            ("network", message)
        }
        TransferError::Reresolve => ("expired", "o link continua expirando".to_owned()),
        TransferError::FileChanged => (
            "changed",
            "o arquivo continua mudando no servidor".to_owned(),
        ),
        TransferError::Disk(m) => ("disk", format!("erro de disco: {m}")),
        TransferError::Fatal(m) => ("fatal", m),
        TransferError::Cancelled => return Ok(Settled::Waiting),
    };
    downloads::fail(c, id, Event::Fail, kind, &message)?;
    history::record(c, id, Outcome::Failed)?;
    Ok(Settled::Failed(message))
}
