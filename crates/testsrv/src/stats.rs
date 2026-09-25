//! Contadores do servidor de teste: picos de conexões e de downloads
//! simultâneos (arquivos distintos com conexão aberta).

use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Retrato dos contadores.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Stats {
    pub requests: u64,
    pub active_connections: u64,
    pub peak_connections: u64,
    pub peak_downloads: u64,
    pub bytes_sent: u64,
}

#[derive(Default)]
struct Inner {
    stats: Stats,
    per_name: HashMap<String, u64>,
}

/// Contadores compartilhados entre as requisições.
#[derive(Clone, Default)]
pub struct Counters(Arc<Mutex<Inner>>);

/// Conexão aberta; ao ser descartada (fim do corpo ou queda) sai dos contadores.
pub struct ConnGuard {
    counters: Counters,
    name: String,
}

impl Counters {
    /// Registra uma requisição que vai enviar corpo.
    pub fn open(&self, name: &str) -> ConnGuard {
        let mut g = self.0.lock().unwrap();
        g.stats.requests += 1;
        g.stats.active_connections += 1;
        *g.per_name.entry(name.to_owned()).or_default() += 1;
        let downloads = g.per_name.values().filter(|n| **n > 0).count() as u64;
        g.stats.peak_connections = g.stats.peak_connections.max(g.stats.active_connections);
        g.stats.peak_downloads = g.stats.peak_downloads.max(downloads);
        ConnGuard {
            counters: self.clone(),
            name: name.to_owned(),
        }
    }

    /// Soma bytes enviados.
    pub fn add_bytes(&self, n: u64) {
        self.0.lock().unwrap().stats.bytes_sent += n;
    }

    /// Retrato atual.
    pub fn snapshot(&self) -> Stats {
        self.0.lock().unwrap().stats.clone()
    }

    /// Zera picos e totais (mantém conexões ativas).
    pub fn reset(&self) {
        let mut g = self.0.lock().unwrap();
        let active = g.stats.active_connections;
        g.stats = Stats {
            active_connections: active,
            ..Stats::default()
        };
    }
}

impl Drop for ConnGuard {
    fn drop(&mut self) {
        let mut g = self.counters.0.lock().unwrap();
        g.stats.active_connections = g.stats.active_connections.saturating_sub(1);
        if let Some(n) = g.per_name.get_mut(&self.name) {
            *n = n.saturating_sub(1);
        }
    }
}
