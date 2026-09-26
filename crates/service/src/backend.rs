//! O serviço visto pelas interfaces: implementa `swoop_api::Backend`,
//! convertendo pedidos em chamadas ao motor e erros internos em mensagens
//! para o usuário.

use crate::{Service, ServiceError};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use swoop_api::{
    AccountsView, ApiError, Backend, CaptchaInfo, CollectorView, Command, DownloadView,
    HistoryOutcome, HistoryView, Reply, Subscription,
};
use swoop_core::{CaptchaChallenge, DownloadId, Settings};
use swoop_store::collector::CollectedRow;
use swoop_store::history::HistoryRow;
use swoop_store::{DownloadRow, StoreError, downloads, packages};

/// Nome dos pacotes criados direto pela API (a janela passa pelo coletor).
const PACKAGE_NAME: &str = "Links adicionados";

#[async_trait]
impl Backend for Service {
    async fn exec(&self, cmd: Command) -> Result<Reply, ApiError> {
        let engine = self.engine();
        match cmd {
            Command::AddLinks { links, dest } => {
                let dest = dest.filter(|d| !d.trim().is_empty()).map(PathBuf::from);
                let ids = self
                    .add_links(&links, dest.as_deref(), PACKAGE_NAME)
                    .await?;
                return Ok(Reply::Added {
                    ids: ids.into_iter().map(|id| id.0).collect(),
                });
            }
            Command::Pause { id } => engine.pause(DownloadId(id)).await.map_err(store)?,
            Command::Resume { id } => engine.resume(DownloadId(id)).await.map_err(store)?,
            Command::Retry { id } => engine.retry(DownloadId(id)).await.map_err(store)?,
            Command::Remove { id, delete_file } => self.spawn_remove(DownloadId(id), delete_file),
            Command::PauseAll => engine.pause_all().await.map_err(store)?,
            Command::ResumeAll => engine.resume_all().await.map_err(store)?,
            Command::SetSpeedLimit { bps } => self.set_speed_limit(bps).await?,
            Command::SaveSettings { settings } => self.save_settings(settings).await?,
            Command::Collect { text } => {
                let added = self.collector.collect(&text).await?;
                return Ok(Reply::Collected { added });
            }
            Command::CollectorStart { ids, dest } => {
                let dest = dest.filter(|d| !d.trim().is_empty()).map(PathBuf::from);
                let ids = self.collector.start(ids, dest.as_deref()).await?;
                return Ok(Reply::Added {
                    ids: ids.into_iter().map(|id| id.0).collect(),
                });
            }
            Command::CollectorRemove { ids } => self.collector.remove(Some(ids)).await?,
            Command::CollectorRemoveOffline => self.collector.remove(None).await?,
            Command::CollectorClear => self.collector.clear().await?,
            Command::AddAccount {
                host,
                kind,
                username,
                secret,
            } => self.add_account(&host, &username, kind, secret).await?,
            Command::RemoveAccount { host } => self.remove_account(&host).await?,
            Command::CheckAccount { host } => {
                self.check_account(&host).await?;
            }
        }
        Ok(Reply::Done)
    }

    async fn list(&self) -> Result<Vec<DownloadView>, ApiError> {
        let (rows, pkgs) = self
            .store
            .call(|c| Ok((downloads::list(c)?, packages::list(c)?)))
            .await
            .map_err(store)?;
        let names: HashMap<i64, String> = pkgs.into_iter().map(|p| (p.id.0, p.name)).collect();
        Ok(rows
            .into_iter()
            .map(|r| {
                let name = names.get(&r.package_id.0).cloned().unwrap_or_default();
                view(r, name)
            })
            .collect())
    }

    async fn collector(&self) -> Result<Vec<CollectorView>, ApiError> {
        let rows = self.collector.rows().await?;
        Ok(rows.into_iter().map(collector_view).collect())
    }

    async fn history(&self, limit: u32) -> Result<Vec<HistoryView>, ApiError> {
        let rows = Service::history(self, limit.clamp(1, 1000)).await?;
        Ok(rows.into_iter().map(history_view).collect())
    }

    async fn settings(&self) -> Result<Settings, ApiError> {
        Ok(Service::settings(self))
    }

    async fn accounts(&self) -> Result<AccountsView, ApiError> {
        let rows = self.account_rows().await?;
        Ok(AccountsView {
            hosts: self.account_hosts(),
            accounts: rows.into_iter().map(crate::accounts::view).collect(),
        })
    }

    fn subscribe(&self) -> Subscription {
        Subscription {
            snapshots: self.engine().snapshots(),
            pushes: self.pushes.subscribe(),
        }
    }
}

impl Service {
    /// Remove em segundo plano: parar um download no meio da conferência de
    /// hash pode demorar, e a interface não deve ficar esperando. O motor
    /// avisa (`Changed`) quando termina.
    fn spawn_remove(&self, id: DownloadId, delete_file: bool) {
        let engine = self.engine().clone();
        tokio::spawn(async move {
            if let Err(e) = engine.remove(id, delete_file).await {
                tracing::warn!(id = id.0, "não consegui remover: {e}");
            }
        });
    }
}

/// Linha do banco → linha da interface.
fn view(r: DownloadRow, package_name: String) -> DownloadView {
    let captcha = r
        .captcha
        .as_deref()
        .and_then(|json| serde_json::from_str::<CaptchaChallenge>(json).ok())
        .map(|c| CaptchaInfo {
            host: c.host().to_owned(),
            not_before_ms: c.not_before_ms,
        });
    DownloadView {
        captcha,
        id: r.id.0,
        package_id: r.package_id.0,
        package_name,
        url: r.url,
        file_name: r.file_name,
        state: r.state,
        size: r.size,
        done_bytes: r.done_bytes,
        error: r.error_msg,
        wait_until_ms: r.wait_until,
        final_path: r.final_path.map(|p| p.to_string_lossy().into_owned()),
    }
}

/// Link do coletor → linha da interface.
fn collector_view(r: CollectedRow) -> CollectorView {
    CollectorView {
        id: r.id,
        url: r.url,
        host: r.host_key,
        state: r.state,
        file_name: r.file_name,
        size: r.size,
        error: r.error,
    }
}

/// Entrada do histórico → linha da interface.
fn history_view(r: HistoryRow) -> HistoryView {
    let outcome = match r.outcome.as_str() {
        "completed" => HistoryOutcome::Completed,
        "removed" => HistoryOutcome::Removed,
        _ => HistoryOutcome::Failed,
    };
    HistoryView {
        id: r.id,
        url: r.url,
        file_name: r.file_name,
        size: r.size,
        final_path: r.final_path.map(|p| p.to_string_lossy().into_owned()),
        outcome,
        error: r.error_msg,
        finished_ms: r.finished_at,
        avg_bps: r.avg_bps,
    }
}

/// Erro do banco/motor → erro para a interface.
fn store(e: StoreError) -> ApiError {
    ServiceError::Store(e).into()
}

impl From<ServiceError> for ApiError {
    fn from(e: ServiceError) -> Self {
        match e {
            ServiceError::BadLink(_) | ServiceError::BadSettings(_) | ServiceError::BadInput(_) => {
                ApiError::Invalid(e.to_string())
            }
            ServiceError::Store(StoreError::NotFound(id)) => {
                ApiError::Invalid(format!("o download {id} não está mais na lista"))
            }
            ServiceError::Store(StoreError::Transition(t)) => ApiError::Invalid(format!(
                "essa ação não vale para um download no estado \"{}\"",
                t.from
            )),
            ServiceError::Busy(_) => ApiError::Unavailable(e.to_string()),
            _ => ApiError::Internal(e.to_string()),
        }
    }
}

#[cfg(test)]
#[path = "backend_tests.rs"]
mod tests;
