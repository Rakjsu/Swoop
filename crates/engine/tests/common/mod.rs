//! Arnês dos testes do motor: banco e pasta temporários, motor com o
//! resolvedor de link direto e esperas por estado final.

#![allow(dead_code)]

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use swoop_core::{DownloadId, DownloadState, PackageId, Resolver, Settings};
use swoop_engine::{Engine, EngineConfig};
use swoop_hosts::DirectResolver;
use swoop_store::{DownloadRow, Store, downloads, packages};

pub const MIB: u64 = 1024 * 1024;

pub struct Harness {
    pub engine: Engine,
    pub store: Store,
    pub dir: tempfile::TempDir,
    pub pkg: PackageId,
    resolver: Arc<dyn Resolver>,
}

impl Harness {
    /// Motor novo com banco e pasta de destino temporários.
    pub async fn new(settings: Settings) -> Self {
        Self::with_resolver(settings, Arc::new(DirectResolver)).await
    }

    /// Mesmo, com outro resolvedor (plugins de teste).
    pub async fn with_resolver(settings: Settings, resolver: Arc<dyn Resolver>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("swoop.sqlite")).unwrap();
        let dest = dir.path().join("downloads");
        let pkg = store
            .call(move |c| packages::insert(c, "teste", &dest, false))
            .await
            .unwrap();
        let engine = start_engine(store.clone(), resolver.clone(), settings);
        Self {
            engine,
            store,
            dir,
            pkg,
            resolver,
        }
    }

    /// Reabre o motor sobre o mesmo banco (simula fechar e abrir o app).
    pub async fn reopen(&mut self, settings: Settings) {
        self.engine.shutdown().await;
        self.store
            .call(|c| downloads::recover_all(c))
            .await
            .unwrap();
        self.engine = start_engine(self.store.clone(), self.resolver.clone(), settings);
    }

    /// Põe um link na fila.
    pub async fn add(&self, url: String) -> DownloadId {
        let pkg = self.pkg;
        let id = self
            .store
            .call(move |c| downloads::insert(c, pkg, &url))
            .await
            .unwrap();
        self.engine.wake();
        id
    }

    pub async fn row(&self, id: DownloadId) -> DownloadRow {
        self.store
            .call(move |c| downloads::get(c, id))
            .await
            .unwrap()
            .unwrap()
    }

    /// Espera até todos os ids ficarem concluídos ou falhos.
    pub async fn wait_final(&self, ids: &[DownloadId], timeout: Duration) -> Vec<DownloadRow> {
        let deadline = Instant::now() + timeout;
        loop {
            let mut rows = Vec::new();
            for id in ids {
                rows.push(self.row(*id).await);
            }
            if rows.iter().all(|r| r.state.is_final()) {
                return rows;
            }
            assert!(
                Instant::now() < deadline,
                "tempo esgotado; estados: {:?}",
                rows.iter()
                    .map(|r| (r.id, r.state, r.error_msg.clone()))
                    .collect::<Vec<_>>()
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Espera até o download ter pelo menos `bytes` confirmados no banco.
    pub async fn wait_done_bytes(&self, id: DownloadId, bytes: u64, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while self.row(id).await.done_bytes < bytes {
            assert!(Instant::now() < deadline, "sem progresso suficiente");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    pub fn dest(&self) -> PathBuf {
        self.dir.path().join("downloads")
    }
}

fn start_engine(store: Store, resolver: Arc<dyn Resolver>, settings: Settings) -> Engine {
    let engine = Engine::new(
        store,
        resolver,
        EngineConfig {
            settings,
            user_agent: "swoop-teste".into(),
        },
    )
    .unwrap();
    engine.start();
    engine
}

/// Preferências para teste com os limites dados.
pub fn settings(conns: u16) -> Settings {
    Settings {
        connections_per_download: conns,
        max_retries: 3,
        ..Settings::default()
    }
}

/// Garante que o download terminou bem e devolve o caminho final.
pub fn completed(row: &DownloadRow) -> PathBuf {
    assert_eq!(
        row.state,
        DownloadState::Completed,
        "download {:?} terminou em {:?}: {:?}",
        row.id,
        row.state,
        row.error_msg
    );
    row.final_path.clone().expect("sem caminho final")
}

pub fn sha256_file(path: &Path) -> String {
    let data = std::fs::read(path).unwrap();
    hex::encode(Sha256::digest(&data))
}
