//! Remoção de um download da fila, inclusive no meio da transferência.
//!
//! Ordem: pausa no banco → marca como "removendo" (o agendador não o inicia
//! mais) e para o job, esperando a escritora fechar o arquivo → histórico e
//! DELETE numa transação → apaga os arquivos. A linha é relida depois do job
//! parar, porque ele pode ter concluído no meio (Downloaded não aceita pausa).

use crate::engine::{Engine, Inner};
use std::path::Path;
use std::sync::Arc;
use swoop_core::{DownloadId, Event};
use swoop_store::{StoreError, downloads};

impl Engine {
    /// Tira o download da fila. O `.part` de um download que não terminou
    /// sempre é apagado; o arquivo final só com `delete_file`. Pode demorar
    /// (espera uma conferência de hash em andamento): rode fora do caminho
    /// da interface.
    pub async fn remove(&self, id: DownloadId, delete_file: bool) -> Result<(), StoreError> {
        let store = &self.inner.store;
        match store
            .call(move |c| downloads::transition(c, id, Event::Pause))
            .await
        {
            Ok(_) | Err(StoreError::Transition(_)) => {}
            Err(e) => return Err(e),
        }

        let (guard, active) = Removing::start(&self.inner, id);
        if let Some(job) = active {
            job.cancel.cancel();
            let _ = job.handle.await;
        }
        let row = store.call(move |c| downloads::remove(c, id)).await?;
        if let Some(row) = row {
            // Sem `final_path` o arquivo nunca foi renomeado: o `.part` é lixo.
            if row.final_path.is_none()
                && let Some(part) = &row.part_path
            {
                remove_file_quiet(part).await;
            }
            if delete_file && let Some(done) = &row.final_path {
                remove_file_quiet(done).await;
            }
        }
        drop(guard);
        self.changed();
        Ok(())
    }
}

/// Marca de "removendo" que some quando a remoção termina (ou falha).
struct Removing {
    inner: Arc<Inner>,
    id: DownloadId,
}

impl Removing {
    /// Marca o id e tira o job ativo do mapa, sob a mesma trava que o
    /// agendador usa para registrar jobs novos.
    fn start(inner: &Arc<Inner>, id: DownloadId) -> (Self, Option<crate::engine::Active>) {
        let mut active = inner.active.lock().expect("mutex");
        inner.removing.lock().expect("mutex").insert(id);
        let job = active.remove(&id);
        let guard = Self {
            inner: inner.clone(),
            id,
        };
        (guard, job)
    }
}

impl Drop for Removing {
    fn drop(&mut self) {
        self.inner.removing.lock().expect("mutex").remove(&self.id);
    }
}

/// Apaga um arquivo; "não existe" não é erro, o resto só vai para o log.
async fn remove_file_quiet(path: &Path) {
    match tokio::fs::remove_file(path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!("não consegui apagar {}: {e}", path.display()),
    }
}
