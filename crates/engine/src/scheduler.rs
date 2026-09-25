//! Agendador: acorda esperas vencidas e inicia downloads da fila respeitando
//! o máximo de downloads simultâneos e de conexões por servidor.

use crate::engine::{Active, Inner};
use crate::job::{self, EngineEvent, JobCtx};
use crate::progress::JobProgress;
use std::sync::Arc;
use std::time::Duration;
use swoop_core::{DownloadId, Event, Settings};
use swoop_store::{DownloadRow, downloads, now_ms};
use url::Url;

/// Sem avisos, o agendador ainda confere a fila neste intervalo (esperas).
const TICK: Duration = Duration::from_millis(500);

/// Candidatos lidos da fila por rodada.
const QUEUE_PEEK: usize = 200;

/// Laço do agendador até o encerramento.
pub async fn run(inner: Arc<Inner>) {
    loop {
        reap(&inner);
        if let Err(e) = fill(&inner).await {
            tracing::error!("agendador: {e}");
        }
        tokio::select! {
            _ = inner.shutdown.cancelled() => return,
            _ = inner.wake.notified() => {}
            _ = tokio::time::sleep(TICK) => {}
        }
    }
}

/// Remove da lista os jobs que já terminaram.
fn reap(inner: &Inner) {
    inner
        .active
        .lock()
        .expect("mutex")
        .retain(|_, a| !a.handle.is_finished());
}

/// Chave de servidor de um download ainda não resolvido.
fn host_of(row: &DownloadRow) -> String {
    row.host_key.clone().unwrap_or_else(|| {
        Url::parse(&row.url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
            .unwrap_or_default()
    })
}

/// Inicia o que couber.
async fn fill(inner: &Arc<Inner>) -> Result<(), swoop_store::StoreError> {
    let woken = inner
        .store
        .call(|c| downloads::wake_due(c, now_ms()))
        .await?;
    if woken > 0 {
        let _ = inner.events_tx.send(EngineEvent::Changed);
    }
    let settings = inner.settings.lock().expect("mutex").clone();
    if free_slots(inner, &settings) == 0 {
        return Ok(());
    }
    let queued = inner
        .store
        .call(|c| downloads::queued(c, QUEUE_PEEK))
        .await?;
    for row in queued {
        if free_slots(inner, &settings) == 0 || inner.shutdown.is_cancelled() {
            break;
        }
        let host = host_of(&row);
        let Some(grant) = grant_for(inner, &settings, &host, row.id) else {
            continue;
        };
        let id = row.id;
        match inner
            .store
            .call(move |c| downloads::transition(c, id, Event::Start))
            .await
        {
            Ok(_) => spawn_job(inner, &settings, id, host, grant),
            Err(e) => tracing::debug!(id = id.0, "não iniciou: {e}"),
        }
    }
    Ok(())
}

/// Vagas de download livres.
fn free_slots(inner: &Inner, settings: &Settings) -> usize {
    let active = inner.active.lock().expect("mutex").len();
    settings.max_active_downloads.saturating_sub(active)
}

/// Conexões que o download pode abrir sem estourar o limite do servidor
/// (`None` = servidor lotado, fica na fila).
fn grant_for(inner: &Inner, settings: &Settings, host: &str, id: DownloadId) -> Option<u16> {
    let active = inner.active.lock().expect("mutex");
    if active.contains_key(&id) {
        return None;
    }
    let used: u16 = active
        .values()
        .filter(|a| a.host == host)
        .map(|a| a.conns)
        .sum();
    let left = settings.max_connections_per_host.saturating_sub(used);
    (left > 0).then(|| left.min(settings.connections_per_download).max(1))
}

/// Sobe a tarefa do job e registra como ativo, a menos que o download esteja
/// sendo removido (a trava do `active` cobre a checagem e o registro juntos).
fn spawn_job(inner: &Arc<Inner>, settings: &Settings, id: DownloadId, host: String, grant: u16) {
    let mut active = inner.active.lock().expect("mutex");
    if inner.removing.lock().expect("mutex").contains(&id) {
        return;
    }
    let progress = Arc::new(JobProgress::default());
    let cancel = inner.shutdown.child_token();
    let ctx = JobCtx {
        store: inner.store.clone(),
        resolver: inner.resolver.clone(),
        client: inner.client.clone(),
        limiter: inner.limiter.clone(),
        settings: settings.clone(),
        granted_conns: grant,
        cancel: cancel.clone(),
        progress: progress.clone(),
        events: inner.events_tx.clone(),
    };
    let waker = inner.clone();
    let _ = inner.events_tx.send(EngineEvent::Started(id));
    tracing::info!(id = id.0, host = %host, conns = grant, "iniciando");
    let handle = tokio::spawn(async move {
        job::run(ctx, id).await;
        waker.wake.notify_one();
    });
    active.insert(
        id,
        Active {
            host,
            conns: grant,
            cancel,
            progress,
            handle,
        },
    );
}
