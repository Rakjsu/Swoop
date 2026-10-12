//! Agendador: acorda esperas vencidas e inicia downloads da fila respeitando
//! o máximo de downloads simultâneos, de conexões por servidor e as regras de
//! cada servidor (`HostPolicy`: um por vez no grátis, esperas do servidor).

use crate::engine::{Active, Inner};
use crate::job::{self, EngineEvent, JobCtx};
use crate::progress::JobProgress;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use swoop_core::{CaptchaChallenge, DownloadId, Event, Settings};
use swoop_store::{DownloadRow, downloads, host_state, now_ms};

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

/// Remove da lista os jobs que terminaram sem sair dela sozinhos (pânico).
fn reap(inner: &Inner) {
    inner
        .active
        .lock()
        .expect("mutex")
        .retain(|_, a| !a.handle.is_finished());
}

/// Inicia o que couber: primeiro quem já respondeu o captcha (o envio tem
/// prazo), depois a fila, página a página (um servidor que não anda não
/// esconde os outros).
async fn fill(inner: &Arc<Inner>) -> Result<(), swoop_store::StoreError> {
    let woken = inner
        .store
        .call(|c| downloads::wake_due(c, now_ms()))
        .await?;
    if woken > 0 {
        let _ = inner.events_tx.send(EngineEvent::Changed);
    }
    if free_slots(inner) == 0 {
        return Ok(());
    }
    let (waiting, pending, answered) = inner
        .store
        .call(|c| {
            let waits = host_state::active(c, now_ms())?;
            let pending = downloads::in_state(c, swoop_core::DownloadState::CaptchaNeeded)?;
            Ok((waits, pending, downloads::queued_answered(c)?))
        })
        .await?;
    let gate = Gate {
        // Servidores que mandaram esperar antes do próximo download.
        blocked: waiting.into_iter().map(|w| w.host_key).collect(),
        held: captcha_held(&pending),
    };
    // Vaga guardada para cada captcha pendente com prazo: a resposta não pode
    // esperar outro download terminar.
    let reserved = pending.iter().filter(|r| live_deadline(r)).count();
    for row in answered {
        if free_slots(inner) == 0 {
            return Ok(());
        }
        try_start(inner, &gate, row).await;
    }
    let mut after = i64::MIN;
    while free_slots(inner) > reserved && !inner.shutdown.is_cancelled() {
        let page = inner
            .store
            .call(move |c| downloads::queued_after(c, after, QUEUE_PEEK))
            .await?;
        let Some(last) = page.last() else {
            break;
        };
        after = last.position;
        for row in page {
            if free_slots(inner) <= reserved || inner.shutdown.is_cancelled() {
                break;
            }
            try_start(inner, &gate, row).await;
        }
    }
    Ok(())
}

/// O que segura um servidor nesta rodada.
struct Gate {
    blocked: HashSet<String>,
    held: HashMap<String, usize>,
}

/// Inicia `row` se o servidor deixar (espera, vaga do servidor, conexões).
/// As preferências são lidas aqui, e não no começo da rodada: uma troca feita
/// enquanto o agendador lia o banco já vale para este início.
async fn try_start(inner: &Arc<Inner>, gate: &Gate, row: DownloadRow) {
    let settings = &current_settings(inner);
    let host = row.server();
    let policy = inner.resolver.host_policy(&host);
    if !policy.ignore_host_waits
        && (gate.blocked.contains(&host) || gate.blocked.contains(&row.url_server()))
    {
        return;
    }
    if let Some(max) = policy.max_active {
        let busy = active_on(inner, &host) + gate.held.get(&host).copied().unwrap_or(0);
        if busy >= max {
            return;
        }
    }
    let Some(grant) = grant_for(inner, settings, &host, row.id) else {
        return;
    };
    let id = row.id;
    match inner
        .store
        .call(move |c| downloads::transition(c, id, Event::Start))
        .await
    {
        Ok(_) => spawn_job(inner, settings, id, host, grant),
        Err(e) => tracing::debug!(id = id.0, "não iniciou: {e}"),
    }
}

/// Captcha com prazo do site ainda correndo.
fn live_deadline(row: &DownloadRow) -> bool {
    row.captcha
        .as_deref()
        .and_then(|j| serde_json::from_str::<CaptchaChallenge>(j).ok())
        .is_some_and(|c| c.expires_at_ms.is_some() && !c.expired(now_ms()))
}

/// Downloads esperando o usuário resolver o captcha, ainda no prazo do site,
/// por servidor: seguram a vaga do servidor (`max_active`).
fn captcha_held(pending: &[DownloadRow]) -> HashMap<String, usize> {
    let mut held = HashMap::new();
    for row in pending.iter().filter(|r| live_deadline(r)) {
        *held.entry(row.server()).or_insert(0) += 1;
    }
    held
}

/// Downloads ativos do servidor.
fn active_on(inner: &Inner, host: &str) -> usize {
    let active = inner.active.lock().expect("mutex");
    active.values().filter(|a| a.host == host).count()
}

/// Preferências atuais do motor.
fn current_settings(inner: &Inner) -> Settings {
    inner.settings.lock().expect("mutex").clone()
}

/// Vagas de download livres, pelo limite atual.
fn free_slots(inner: &Inner) -> usize {
    let max = inner.settings.lock().expect("mutex").max_active_downloads;
    let active = inner.active.lock().expect("mutex").len();
    max.saturating_sub(active)
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
        host: host.clone(),
        cancel: cancel.clone(),
        progress: progress.clone(),
        events: inner.events_tx.clone(),
    };
    let waker = inner.clone();
    let _ = inner.events_tx.send(EngineEvent::Started(id));
    tracing::info!(id = id.0, host = %host, conns = grant, "iniciando");
    let handle = tokio::spawn(async move {
        job::run(ctx, id).await;
        // Sai do mapa antes de avisar: a vaga e as conexões ficam livres já
        // nesta volta do agendador, sem esperar a tarefa acabar de fechar.
        // Um job novo do mesmo id só entra depois que este saiu (`grant_for`).
        waker.active.lock().expect("mutex").remove(&id);
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
