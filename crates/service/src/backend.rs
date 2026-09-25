//! O serviço visto pelas interfaces: implementa `swoop_api::Backend`,
//! convertendo pedidos em chamadas ao motor e erros internos em mensagens
//! para o usuário.

use crate::{Service, ServiceError};
use async_trait::async_trait;
use std::path::PathBuf;
use swoop_api::{
    ApiError, Backend, Command, DownloadView, HistoryOutcome, HistoryView, Reply, Subscription,
};
use swoop_core::{DownloadId, Settings};
use swoop_store::history::HistoryRow;
use swoop_store::{DownloadRow, StoreError};

/// Nome dos pacotes criados pela janela (o agrupamento real vem na fase 3).
const PACKAGE_NAME: &str = "Adicionados pela janela";

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
        }
        Ok(Reply::Done)
    }

    async fn list(&self) -> Result<Vec<DownloadView>, ApiError> {
        Ok(self.rows().await?.into_iter().map(view).collect())
    }

    async fn history(&self, limit: u32) -> Result<Vec<HistoryView>, ApiError> {
        let rows = Service::history(self, limit.clamp(1, 1000)).await?;
        Ok(rows.into_iter().map(history_view).collect())
    }

    async fn settings(&self) -> Result<Settings, ApiError> {
        Ok(Service::settings(self))
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
fn view(r: DownloadRow) -> DownloadView {
    DownloadView {
        id: r.id.0,
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
            ServiceError::BadLink(_) | ServiceError::BadSettings(_) => {
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
