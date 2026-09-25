//! Coletor de links, como o do Mipony: o texto colado vira links (pastas já
//! abertas), uma tarefa confere cada um no servidor (até `MAX_CHECKS` ao
//! mesmo tempo, um por servidor) e o "Iniciar" manda os escolhidos para a
//! fila num pacote. Com "iniciar sozinho", cada colagem vai para a fila
//! quando termina de ser conferida.

use crate::{ServiceError, links, naming};
use std::path::Path;
use std::sync::Arc;
use swoop_api::Push;
use swoop_core::filename::file_name_from_url;
use swoop_core::{DownloadId, HostError, LinkState};
use swoop_engine::Engine;
use swoop_hosts::Registry;
use swoop_store::Store;
use swoop_store::collector::{self, CollectedRow};
use tokio::sync::{Notify, broadcast};
use tokio::task::{JoinHandle, JoinSet};
use url::Url;

/// Verificações ao mesmo tempo (servidores diferentes).
const MAX_CHECKS: usize = 4;

/// O coletor e o que ele usa. Barato de clonar (tudo compartilhado).
#[derive(Clone)]
pub struct Collector {
    store: Store,
    hosts: Arc<Registry>,
    engine: Engine,
    pushes: broadcast::Sender<Push>,
    wake: Arc<Notify>,
}

impl Collector {
    pub fn new(
        store: Store,
        hosts: Arc<Registry>,
        engine: Engine,
        pushes: broadcast::Sender<Push>,
    ) -> Self {
        Self {
            store,
            hosts,
            engine,
            pushes,
            wake: Arc::new(Notify::new()),
        }
    }

    /// Liga a verificação em segundo plano (retoma a que o app fechou no meio).
    pub fn spawn(&self) -> JoinHandle<()> {
        let me = self.clone();
        me.wake.notify_one();
        tokio::spawn(async move { me.run().await })
    }

    /// Acha os links do texto e guarda os novos; devolve quantos entraram.
    pub async fn collect(&self, text: &str) -> Result<u32, ServiceError> {
        let found = self.hosts.detect(text);
        if found.is_empty() {
            return Err(ServiceError::BadLink("nenhum link no texto".into()));
        }
        let urls = links::expand_all(&self.hosts, found).await;
        let keyed: Vec<(String, String)> = urls
            .iter()
            .map(|u| (u.to_string(), self.hosts.host_key(u)))
            .collect();
        let added = self
            .store
            .call(move |c| {
                let batch = collector::next_batch(c)?;
                let mut n = 0;
                for (url, host) in &keyed {
                    n += u32::from(collector::insert(c, url, host, batch)?);
                }
                Ok(n)
            })
            .await?;
        self.wake.notify_one();
        self.changed();
        Ok(added)
    }

    /// Manda os links escolhidos (menos offline e em verificação) para a
    /// fila num pacote com nome tirado dos arquivos.
    pub async fn start(
        &self,
        ids: Vec<i64>,
        dest: Option<&Path>,
    ) -> Result<Vec<DownloadId>, ServiceError> {
        let rows = self.store.call(move |c| collector::take(c, &ids)).await?;
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let names: Vec<String> = rows.iter().filter_map(display_name).collect();
        let urls = rows.into_iter().map(|r| r.url).collect();
        let name = naming::package_name(&names);
        let ids = links::enqueue(&self.store, &self.engine, urls, dest, &name).await?;
        self.changed();
        Ok(ids)
    }

    /// Links do coletor, na ordem em que entraram.
    pub async fn rows(&self) -> Result<Vec<CollectedRow>, ServiceError> {
        Ok(self.store.call(|c| collector::list(c)).await?)
    }

    /// Tira links do coletor (`None` = só os offline).
    pub async fn remove(&self, ids: Option<Vec<i64>>) -> Result<(), ServiceError> {
        self.store
            .call(move |c| match ids {
                Some(ids) => collector::remove(c, &ids),
                None => collector::remove_offline(c),
            })
            .await?;
        self.changed();
        Ok(())
    }

    /// Esvazia o coletor.
    pub async fn clear(&self) -> Result<(), ServiceError> {
        self.store.call(|c| collector::clear(c)).await?;
        self.changed();
        Ok(())
    }

    fn changed(&self) {
        let _ = self.pushes.send(Push::Changed);
    }

    /// Laço de verificação: pega uma leva, confere em paralelo, repete.
    async fn run(self) {
        loop {
            let claimed = match self.store.call(|c| collector::claim(c, MAX_CHECKS)).await {
                Ok(rows) => rows,
                Err(e) => {
                    tracing::warn!("coletor parou: {e}");
                    return;
                }
            };
            if claimed.is_empty() {
                self.wake.notified().await;
                continue;
            }
            self.changed();
            let mut set = JoinSet::new();
            for row in claimed {
                let me = self.clone();
                set.spawn(async move { me.check(row).await });
            }
            while set.join_next().await.is_some() {}
        }
    }

    /// Confere um link e grava o resultado; fecha a colagem se for o caso.
    async fn check(&self, row: CollectedRow) {
        let result = match Url::parse(&row.url) {
            Ok(url) => self.hosts.check(&url).await,
            Err(_) => Err(HostError::Unsupported),
        };
        let (state, name, size, error) = match result {
            Ok(info) => (LinkState::Online, info.name, info.size, None),
            Err(HostError::Offline) => (
                LinkState::Offline,
                None,
                None,
                Some(HostError::Offline.to_string()),
            ),
            Err(e) => (LinkState::Failed, None, None, Some(describe(&e))),
        };
        let id = row.id;
        let saved = self
            .store
            .call(move |c| {
                collector::set_result(c, id, state, name.as_deref(), size, error.as_deref())
            })
            .await;
        if let Err(e) = saved {
            tracing::warn!("coletor: não gravei a verificação: {e}");
        }
        self.changed();
        if self.engine.settings().auto_start {
            self.auto_start(row.batch).await;
        }
    }

    /// "Iniciar sozinho": colagem toda conferida → os online viram um pacote.
    async fn auto_start(&self, batch: i64) {
        let ids = self
            .store
            .call(move |c| {
                if collector::batch_pending(c, batch)? {
                    Ok(Vec::new())
                } else {
                    collector::online_in_batch(c, batch)
                }
            })
            .await;
        match ids {
            Ok(ids) if !ids.is_empty() => {
                if let Err(e) = self.start(ids, None).await {
                    tracing::warn!("coletor: iniciar sozinho falhou: {e}");
                }
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("coletor: {e}"),
        }
    }
}

/// Nome conhecido do arquivo ou o último trecho do link.
fn display_name(row: &CollectedRow) -> Option<String> {
    row.file_name.clone().or_else(|| {
        Url::parse(&row.url)
            .ok()
            .and_then(|u| file_name_from_url(&u))
    })
}

/// Motivo em pt-BR (espera diz qual).
fn describe(e: &HostError) -> String {
    match e {
        HostError::Wait { reason, .. } => format!("aguardando: {}", reason.describe()),
        e => e.to_string(),
    }
}

#[cfg(test)]
#[path = "collector_tests.rs"]
mod tests;
