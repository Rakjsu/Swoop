//! Progresso para as interfaces: contadores atômicos por download e o
//! "retrato" agregado publicado algumas vezes por segundo (os tipos do
//! retrato são os do contrato `swoop-api`, que a UI recebe como estão).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Instant;
use swoop_api::{LiveRow, Snapshot};
use swoop_core::DownloadId;

/// Contadores de um download ativo (escritos pelas conexões).
#[derive(Debug, Default)]
pub struct JobProgress {
    /// Bytes recebidos (inclui sessões anteriores).
    pub received: AtomicU64,
    /// Tamanho total (0 = desconhecido).
    pub total: AtomicU64,
    /// Conexões abertas agora.
    pub conns: AtomicU32,
}

/// Mede velocidade por download com média móvel exponencial.
#[derive(Default)]
pub struct SpeedMeter {
    seq: u64,
    last: HashMap<DownloadId, (Instant, u64, f64)>,
}

/// Peso da amostra nova na média móvel.
const ALPHA: f64 = 0.3;

impl SpeedMeter {
    /// Monta o retrato a partir dos contadores atuais.
    pub fn snapshot(
        &mut self,
        jobs: &[(DownloadId, &JobProgress)],
        limit_bps: Option<u64>,
        now: Instant,
    ) -> Snapshot {
        self.seq += 1;
        let mut next = HashMap::with_capacity(jobs.len());
        let mut active = Vec::with_capacity(jobs.len());
        for (id, p) in jobs {
            let received = p.received.load(Ordering::Relaxed);
            let total = Some(p.total.load(Ordering::Relaxed)).filter(|t| *t > 0);
            let ema = match self.last.get(id) {
                Some((t0, r0, ema)) => {
                    let dt = now.duration_since(*t0).as_secs_f64();
                    if dt > 0.0 {
                        let inst = received.saturating_sub(*r0) as f64 / dt;
                        ALPHA * inst + (1.0 - ALPHA) * ema
                    } else {
                        *ema
                    }
                }
                None => 0.0,
            };
            next.insert(*id, (now, received, ema));
            let speed_bps = ema.round() as u64;
            let eta_secs = total
                .filter(|_| speed_bps > 0)
                .map(|t| t.saturating_sub(received) / speed_bps);
            active.push(LiveRow {
                id: id.0,
                received,
                total,
                speed_bps,
                conns: p.conns.load(Ordering::Relaxed),
                eta_secs,
            });
        }
        self.last = next;
        Snapshot {
            seq: self.seq,
            speed_bps: active.iter().map(|r| r.speed_bps).sum(),
            limit_bps,
            active,
        }
    }
}
