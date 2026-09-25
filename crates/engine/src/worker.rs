//! Uma conexão de um download: pega uma faixa, pede com Range (+ If-Range),
//! passa cada pedaço pelo limitador e entrega à escritora. Quando a faixa
//! acaba, divide a de outra conexão; quando não há mais o que dividir, sai.

use crate::error::TransferError;
use crate::progress::JobProgress;
use crate::request;
use crate::table::{Claim, Table, WorkerId};
use crate::writer::WriteJob;
use async_speed_limit::Limiter;
use futures_util::StreamExt;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use swoop_core::{ErrorClass, HttpFailure, Resolved, Resolver, WaitReason};
use swoop_net::reqwest::{self, StatusCode, header};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Espera acima disso vira espera do download inteiro (sai da fila de conexões).
const LONG_WAIT: Duration = Duration::from_secs(60);

/// Tudo que as conexões de um download compartilham.
pub struct WorkerCtx {
    pub client: reqwest::Client,
    pub resolved: Arc<Resolved>,
    pub resolver: Arc<dyn Resolver>,
    /// Valor de If-Range (ETag forte ou Last-Modified).
    pub validator: Option<String>,
    /// `false`: uma requisição sem Range, do começo ao fim.
    pub ranged: bool,
    pub table: Arc<Mutex<Table>>,
    pub writer: mpsc::Sender<WriteJob>,
    pub limiter: Limiter,
    pub cancel: CancellationToken,
    pub max_retries: u32,
    pub progress: Arc<JobProgress>,
}

/// Como terminou uma requisição que pode ser repetida.
enum Outcome {
    Done,
    Retry {
        after: Option<Duration>,
        message: String,
        progressed: bool,
    },
}

/// Laço de uma conexão até não haver mais faixas.
pub async fn run(ctx: Arc<WorkerCtx>, worker: WorkerId) -> Result<(), TransferError> {
    let mut failures: u32 = 0;
    loop {
        if ctx.cancel.is_cancelled() {
            return Err(TransferError::Cancelled);
        }
        let claim = {
            let mut t = ctx.table.lock().expect("tabela envenenada");
            let now = Instant::now();
            t.claim(worker, now)
                .or_else(|| ctx.ranged.then(|| t.steal(worker, now)).flatten())
        };
        let Some(claim) = claim else {
            return Ok(());
        };

        ctx.progress.conns.fetch_add(1, Ordering::Relaxed);
        let result = fetch(&ctx, claim).await;
        ctx.progress.conns.fetch_sub(1, Ordering::Relaxed);

        match result {
            Ok(Outcome::Done) => failures = 0,
            Ok(Outcome::Retry {
                after,
                message,
                progressed,
            }) => {
                release(&ctx, claim.idx);
                failures = if progressed { 1 } else { failures + 1 };
                if failures > ctx.max_retries {
                    return Err(TransferError::Transient(format!(
                        "{message} (desistindo após {failures} tentativas)"
                    )));
                }
                let delay = after.unwrap_or_else(|| backoff(failures));
                if delay > LONG_WAIT {
                    return Err(TransferError::Wait {
                        until: SystemTime::now() + delay,
                        reason: WaitReason::Backoff,
                        message,
                    });
                }
                tracing::debug!(worker, failures, ?delay, "nova tentativa: {message}");
                tokio::select! {
                    _ = ctx.cancel.cancelled() => return Err(TransferError::Cancelled),
                    _ = tokio::time::sleep(delay) => {}
                }
            }
            Err(e) => {
                release(&ctx, claim.idx);
                return Err(e);
            }
        }
    }
}

/// Espera exponencial: 1 s, 2 s, 4 s… até 30 s.
fn backoff(failures: u32) -> Duration {
    Duration::from_secs(1u64 << failures.saturating_sub(1).min(5)).min(Duration::from_secs(30))
}

/// Solta a faixa; sem Range, ela recomeça do zero.
fn release(ctx: &WorkerCtx, idx: u32) {
    let mut t = ctx.table.lock().expect("tabela envenenada");
    t.release(idx);
    if !ctx.ranged {
        t.restart(idx);
        ctx.progress.received.store(t.received(), Ordering::Relaxed);
    }
}

/// Uma requisição da faixa `claim` até ela acabar, ser cortada ou falhar.
async fn fetch(ctx: &WorkerCtx, claim: Claim) -> Result<Outcome, TransferError> {
    let mut req = request::base(&ctx.client, &ctx.resolved);
    if ctx.ranged {
        req = req.header(
            header::RANGE,
            format!("bytes={}-{}", claim.from, claim.to - 1),
        );
        if let Some(v) = &ctx.validator {
            req = req.header(header::IF_RANGE, v.as_str());
        }
    }
    let sent = tokio::select! {
        _ = ctx.cancel.cancelled() => return Err(TransferError::Cancelled),
        r = req.send() => r,
    };
    let resp = match sent {
        Ok(r) => r,
        Err(e) => return Ok(retry(None, request::describe(&e), false)),
    };
    if let Some(outcome) = check_status(ctx, &resp, claim)? {
        return Ok(outcome);
    }

    let mut stream = resp.bytes_stream();
    let mut progressed = false;
    loop {
        let item = tokio::select! {
            _ = ctx.cancel.cancelled() => return Err(TransferError::Cancelled),
            item = stream.next() => item,
        };
        let chunk = match item {
            Some(Ok(chunk)) => chunk,
            Some(Err(e)) => return Ok(retry(None, request::describe(&e), progressed)),
            None => return Ok(end_of_body(ctx, claim, progressed)),
        };
        tokio::select! {
            _ = ctx.cancel.cancelled() => return Err(TransferError::Cancelled),
            _ = ctx.limiter.consume(chunk.len()) => {}
        }
        let adv = {
            let mut t = ctx.table.lock().expect("tabela envenenada");
            let adv = t.advance(claim.idx, chunk.len());
            ctx.progress.received.store(t.received(), Ordering::Relaxed);
            adv
        };
        if adv.accepted > 0 {
            let job = WriteJob {
                idx: claim.idx,
                offset: adv.offset,
                data: chunk.slice(..adv.accepted),
            };
            if ctx.writer.send(job).await.is_err() {
                return Err(TransferError::Disk("a gravação em disco parou".into()));
            }
            progressed = true;
        }
        if adv.finished {
            return Ok(Outcome::Done);
        }
    }
}

/// Confere status e Content-Range. `Some(outcome)` = não ler o corpo.
fn check_status(
    ctx: &WorkerCtx,
    resp: &reqwest::Response,
    claim: Claim,
) -> Result<Option<Outcome>, TransferError> {
    let status = resp.status();
    let expected = if ctx.ranged {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };
    if status != expected {
        if ctx.ranged && status == StatusCode::OK {
            // Range/If-Range ignorado: o arquivo mudou (ou o servidor deixou de aceitar Range).
            return Err(TransferError::FileChanged);
        }
        let f = request::status_failure(status, resp.headers(), SystemTime::now());
        return classify(ctx, &f).map(Some);
    }
    if ctx.ranged {
        let start = resp
            .headers()
            .get(header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(swoop_net::parse_content_range)
            .map(|r| r.start);
        if start != Some(claim.from) {
            return Err(TransferError::Fatal(format!(
                "o servidor devolveu a faixa errada (pedido {}, recebido {start:?})",
                claim.from
            )));
        }
    }
    Ok(None)
}

/// O corpo acabou: sem Range e tamanho desconhecido, é o fim do arquivo;
/// em qualquer outro caso a conexão caiu antes da hora.
fn end_of_body(ctx: &WorkerCtx, claim: Claim, progressed: bool) -> Outcome {
    if !ctx.ranged && claim.to == u64::MAX {
        let total = ctx
            .table
            .lock()
            .expect("tabela envenenada")
            .finish_at_cursor(claim.idx);
        ctx.progress.total.store(total, Ordering::Relaxed);
        return Outcome::Done;
    }
    retry(
        None,
        "o servidor fechou a conexão antes do fim".into(),
        progressed,
    )
}

/// Pede ao resolvedor o destino de uma falha HTTP.
fn classify(ctx: &WorkerCtx, f: &HttpFailure) -> Result<Outcome, TransferError> {
    let message = request::failure_message(f);
    match ctx.resolver.classify(&ctx.resolved.host_key, f) {
        ErrorClass::Retry { after } => Ok(Outcome::Retry {
            after,
            message,
            progressed: false,
        }),
        ErrorClass::Reresolve => Err(TransferError::Reresolve),
        ErrorClass::Wait { until, reason } => Err(TransferError::Wait {
            until,
            reason,
            message,
        }),
        ErrorClass::Fatal(m) => Err(TransferError::Fatal(m)),
    }
}

fn retry(after: Option<Duration>, message: String, progressed: bool) -> Outcome {
    Outcome::Retry {
        after,
        message,
        progressed,
    }
}
