//! API pública do motor: sobe o agendador, recebe comandos (pausar, retomar,
//! limite de velocidade) e publica progresso e eventos.

use crate::job::EngineEvent;
use crate::progress::{JobProgress, Snapshot, SpeedMeter};
use crate::scheduler;
use async_speed_limit::Limiter;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use swoop_core::{DownloadId, Event, Resolver, Settings};
use swoop_net::reqwest;
use swoop_store::{Store, StoreError, downloads};
use tokio::sync::{Notify, broadcast, watch};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Intervalo entre retratos de progresso (≈ 5 por segundo).
const SNAPSHOT_EVERY: Duration = Duration::from_millis(200);

/// Configuração inicial do motor.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub settings: Settings,
    pub user_agent: String,
}

/// Download em andamento, visto pelo agendador.
pub(crate) struct Active {
    pub host: String,
    pub conns: u16,
    pub cancel: CancellationToken,
    pub progress: Arc<JobProgress>,
    pub handle: JoinHandle<()>,
}

pub(crate) struct Inner {
    pub store: Store,
    pub resolver: Arc<dyn Resolver>,
    pub client: reqwest::Client,
    pub limiter: Limiter,
    pub settings: Mutex<Settings>,
    pub wake: Notify,
    pub active: Mutex<HashMap<DownloadId, Active>>,
    pub snapshot_tx: watch::Sender<Snapshot>,
    pub events_tx: broadcast::Sender<EngineEvent>,
    pub shutdown: CancellationToken,
}

/// Handle clonável do motor.
#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

/// Converte o limite em bytes/s para o limitador (sem limite = infinito).
fn limit_value(bps: Option<u64>) -> f64 {
    bps.filter(|b| *b > 0).map_or(f64::INFINITY, |b| b as f64)
}

impl Engine {
    /// Cria o motor (ainda parado; ver `start`).
    pub fn new(
        store: Store,
        resolver: Arc<dyn Resolver>,
        cfg: EngineConfig,
    ) -> Result<Self, reqwest::Error> {
        let client = swoop_net::download_client(&cfg.user_agent)?;
        let limiter = Limiter::new(limit_value(cfg.settings.speed_limit_bps));
        let (snapshot_tx, _) = watch::channel(Snapshot::default());
        let (events_tx, _) = broadcast::channel(256);
        Ok(Self {
            inner: Arc::new(Inner {
                store,
                resolver,
                client,
                limiter,
                settings: Mutex::new(cfg.settings),
                wake: Notify::new(),
                active: Mutex::new(HashMap::new()),
                snapshot_tx,
                events_tx,
                shutdown: CancellationToken::new(),
            }),
        })
    }

    /// Sobe o agendador e o publicador de progresso.
    pub fn start(&self) {
        tokio::spawn(scheduler::run(self.inner.clone()));
        tokio::spawn(publish_snapshots(self.inner.clone()));
    }

    /// Avisa que há novidade na fila (links adicionados, esperas mudadas).
    pub fn wake(&self) {
        self.inner.wake.notify_one();
    }

    /// Pausa: grava o estado e para as conexões (a escritora faz checkpoint).
    pub async fn pause(&self, id: DownloadId) -> Result<(), StoreError> {
        self.inner
            .store
            .call(move |c| downloads::transition(c, id, Event::Pause))
            .await?;
        if let Some(a) = self.inner.active.lock().expect("mutex").get(&id) {
            a.cancel.cancel();
        }
        self.wake();
        Ok(())
    }

    /// Retoma um download pausado (volta para a fila).
    pub async fn resume(&self, id: DownloadId) -> Result<(), StoreError> {
        self.inner
            .store
            .call(move |c| downloads::transition(c, id, Event::Resume))
            .await?;
        self.wake();
        Ok(())
    }

    /// Nova tentativa de um download que falhou.
    pub async fn retry(&self, id: DownloadId) -> Result<(), StoreError> {
        self.inner
            .store
            .call(move |c| {
                downloads::set_attempts(c, id, 0)?;
                downloads::transition(c, id, Event::Retry)
            })
            .await?;
        self.wake();
        Ok(())
    }

    /// Muda o limite global de velocidade na hora (`None` = sem limite).
    pub fn set_speed_limit(&self, bps: Option<u64>) {
        self.inner.limiter.set_speed_limit(limit_value(bps));
        self.inner.settings.lock().expect("mutex").speed_limit_bps = bps;
    }

    /// Troca as preferências (valem para os próximos downloads iniciados).
    pub fn update_settings(&self, settings: Settings) {
        self.set_speed_limit(settings.speed_limit_bps);
        *self.inner.settings.lock().expect("mutex") = settings;
        self.wake();
    }

    /// Preferências atuais.
    pub fn settings(&self) -> Settings {
        self.inner.settings.lock().expect("mutex").clone()
    }

    /// Retratos de progresso (≈ 5 Hz).
    pub fn snapshots(&self) -> watch::Receiver<Snapshot> {
        self.inner.snapshot_tx.subscribe()
    }

    /// Eventos discretos (concluído, falhou, esperando…).
    pub fn events(&self) -> broadcast::Receiver<EngineEvent> {
        self.inner.events_tx.subscribe()
    }

    /// Quantos downloads estão ativos agora.
    pub fn active_count(&self) -> usize {
        self.inner.active.lock().expect("mutex").len()
    }

    /// Para tudo e espera as escritoras gravarem o último checkpoint. Os
    /// downloads ficam `downloading` no banco e voltam para a fila ao reabrir.
    pub async fn shutdown(&self) {
        self.inner.shutdown.cancel();
        let handles: Vec<JoinHandle<()>> = self
            .inner
            .active
            .lock()
            .expect("mutex")
            .drain()
            .map(|(_, a)| a.handle)
            .collect();
        for h in handles {
            let _ = h.await;
        }
    }
}

/// Publica o retrato de progresso periodicamente.
async fn publish_snapshots(inner: Arc<Inner>) {
    let mut meter = SpeedMeter::default();
    let mut tick = tokio::time::interval(SNAPSHOT_EVERY);
    loop {
        tokio::select! {
            _ = inner.shutdown.cancelled() => return,
            _ = tick.tick() => {}
        }
        let jobs: Vec<(DownloadId, Arc<JobProgress>)> = inner
            .active
            .lock()
            .expect("mutex")
            .iter()
            .map(|(id, a)| (*id, a.progress.clone()))
            .collect();
        let refs: Vec<(DownloadId, &JobProgress)> =
            jobs.iter().map(|(id, p)| (*id, p.as_ref())).collect();
        let limit = inner.settings.lock().expect("mutex").speed_limit_bps;
        let snap = meter.snapshot(&refs, limit, Instant::now());
        inner.snapshot_tx.send_replace(snap);
    }
}
