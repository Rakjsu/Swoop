//! Tabela viva de segmentos de um download em andamento.
//!
//! Compartilhada (sob `Mutex`) entre as conexões e a thread escritora:
//! - a conexão avança `cursor` ANTES de mandar os bytes para a escritora, e
//!   `advance` corta o pedaço no `end` atual. Por isso uma divisão (que só
//!   mexe em `end` e nunca abaixo do `cursor`) não gera bytes duplicados;
//! - a escritora avança `written` depois de gravar; o checkpoint persiste
//!   `written`, nunca o `cursor`.
//!
//! Invariante: as faixas `[start, end)` são disjuntas e cobrem o arquivo, com
//! `start ≤ written ≤ cursor ≤ end` em cada uma.

use crate::segments::{Span, split_point};
use std::time::{Duration, Instant};
use swoop_store::SegmentRow;

/// Índice da conexão (worker) dentro do download.
pub type WorkerId = usize;

/// Tempo mínimo de medição antes de chamar uma conexão de lenta.
const SLOW_AFTER: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
struct Seg {
    idx: u32,
    start: u64,
    end: u64,
    cursor: u64,
    written: u64,
    owner: Option<WorkerId>,
    since: Instant,
    since_cursor: u64,
}

impl Seg {
    fn remaining(&self) -> u64 {
        self.end.saturating_sub(self.cursor)
    }

    /// Bytes/s do dono atual (None se medido há pouco tempo).
    fn speed(&self, now: Instant) -> Option<f64> {
        let secs = now.duration_since(self.since);
        (secs >= SLOW_AFTER).then(|| (self.cursor - self.since_cursor) as f64 / secs.as_secs_f64())
    }
}

/// Faixa atribuída a uma conexão: pedir `[from, to)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Claim {
    pub idx: u32,
    pub from: u64,
    pub to: u64,
}

/// Resultado de `advance`: onde gravar, quanto aceitar e se a faixa acabou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Advance {
    pub offset: u64,
    pub accepted: usize,
    pub finished: bool,
}

/// Segmentos vivos de um download.
#[derive(Debug)]
pub struct Table {
    segs: Vec<Seg>,
    next_idx: u32,
}

impl Table {
    /// Monta a partir do que estava salvo (retomada): cursor = pos confirmado.
    pub fn from_rows(rows: &[SegmentRow]) -> Self {
        let now = Instant::now();
        let segs: Vec<Seg> = rows
            .iter()
            .map(|r| {
                let pos = r.pos.clamp(r.start, r.end);
                Seg {
                    idx: r.idx,
                    start: r.start,
                    end: r.end,
                    cursor: pos,
                    written: pos,
                    owner: None,
                    since: now,
                    since_cursor: pos,
                }
            })
            .collect();
        let next_idx = segs.iter().map(|s| s.idx + 1).max().unwrap_or(0);
        Self { segs, next_idx }
    }

    /// Monta a partir de um plano novo.
    pub fn from_spans(spans: &[Span]) -> Self {
        let rows: Vec<SegmentRow> = spans
            .iter()
            .enumerate()
            .map(|(i, s)| SegmentRow {
                idx: i as u32,
                start: s.start,
                end: s.end,
                pos: s.start,
            })
            .collect();
        Self::from_rows(&rows)
    }

    /// Estado para persistir (pos = bytes confirmados pela escritora).
    pub fn rows(&self) -> Vec<SegmentRow> {
        let mut rows: Vec<SegmentRow> = self
            .segs
            .iter()
            .map(|s| SegmentRow {
                idx: s.idx,
                start: s.start,
                end: s.end,
                pos: s.written,
            })
            .collect();
        rows.sort_by_key(|r| r.start);
        rows
    }

    /// Bytes já recebidos (inclui os de sessões anteriores).
    pub fn received(&self) -> u64 {
        self.segs.iter().map(|s| s.cursor - s.start).sum()
    }

    /// Tudo gravado?
    pub fn is_complete(&self) -> bool {
        self.segs.iter().all(|s| s.written >= s.end)
    }

    /// Faixas com algo a baixar.
    #[cfg(test)]
    pub fn pending(&self) -> usize {
        self.segs.iter().filter(|s| s.remaining() > 0).count()
    }

    /// Pega uma faixa sem dono com bytes faltando.
    pub fn claim(&mut self, worker: WorkerId, now: Instant) -> Option<Claim> {
        let seg = self
            .segs
            .iter_mut()
            .find(|s| s.owner.is_none() && s.remaining() > 0)?;
        seg.owner = Some(worker);
        seg.since = now;
        seg.since_cursor = seg.cursor;
        Some(Claim {
            idx: seg.idx,
            from: seg.cursor,
            to: seg.end,
        })
    }

    /// Divide a faixa em andamento com mais bytes faltando e entrega a parte
    /// de trás para `worker`. Vítima lenta perde quase tudo.
    pub fn steal(&mut self, worker: WorkerId, now: Instant) -> Option<Claim> {
        let best = self
            .segs
            .iter()
            .filter(|s| s.owner.is_some() && s.remaining() > 0)
            .filter_map(|s| s.speed(now))
            .fold(0.0_f64, f64::max);
        let victim = self
            .segs
            .iter_mut()
            .filter(|s| s.owner.is_some() && s.owner != Some(worker) && s.remaining() > 0)
            .max_by_key(|s| s.remaining())?;
        let slow = victim
            .speed(now)
            .is_some_and(|v| best > 0.0 && v * 4.0 < best);
        let cut = split_point(victim.cursor, victim.end, slow)?;
        let old_end = victim.end;
        victim.end = cut;

        let idx = self.next_idx;
        self.next_idx += 1;
        self.segs.push(Seg {
            idx,
            start: cut,
            end: old_end,
            cursor: cut,
            written: cut,
            owner: Some(worker),
            since: now,
            since_cursor: cut,
        });
        Some(Claim {
            idx,
            from: cut,
            to: old_end,
        })
    }

    /// Registra `n` bytes recebidos na faixa `idx`, cortando no `end` atual.
    pub fn advance(&mut self, idx: u32, n: usize) -> Advance {
        let Some(seg) = self.segs.iter_mut().find(|s| s.idx == idx) else {
            return Advance {
                offset: 0,
                accepted: 0,
                finished: true,
            };
        };
        let offset = seg.cursor;
        let accepted = (n as u64).min(seg.remaining()) as usize;
        seg.cursor += accepted as u64;
        Advance {
            offset,
            accepted,
            finished: seg.remaining() == 0,
        }
    }

    /// A conexão largou a faixa (erro): outra pode pegá-la.
    pub fn release(&mut self, idx: u32) {
        if let Some(seg) = self.segs.iter_mut().find(|s| s.idx == idx) {
            seg.owner = None;
        }
    }

    /// Download sem Range: recomeça a faixa do zero (os bytes serão regravados).
    pub fn restart(&mut self, idx: u32) {
        if let Some(seg) = self.segs.iter_mut().find(|s| s.idx == idx) {
            seg.cursor = seg.start;
            seg.written = seg.start;
            seg.since_cursor = seg.start;
        }
    }

    /// Download de tamanho desconhecido chegou ao fim em `cursor`.
    pub fn finish_at_cursor(&mut self, idx: u32) -> u64 {
        match self.segs.iter_mut().find(|s| s.idx == idx) {
            Some(seg) => {
                seg.end = seg.cursor;
                seg.end
            }
            None => 0,
        }
    }

    /// A escritora gravou até `upto` na faixa `idx`.
    pub fn mark_written(&mut self, idx: u32, upto: u64) {
        if let Some(seg) = self.segs.iter_mut().find(|s| s.idx == idx) {
            seg.written = seg.written.max(upto.min(seg.end));
        }
    }

    /// Fim da última faixa (tamanho do arquivo).
    pub fn total(&self) -> u64 {
        self.segs.iter().map(|s| s.end).max().unwrap_or(0)
    }
}

#[cfg(test)]
#[path = "table_tests.rs"]
mod tests;
