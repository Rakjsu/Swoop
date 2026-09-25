//! Traduz os eventos do motor para as interfaces: `Push::Changed` (uma vez
//! por rajada) e avisos (`Push::Notice`) quando um download falha de vez ou
//! a fila termina.

use swoop_api::{Notice, Push};
use swoop_core::DownloadId;
use swoop_core::filename::file_name_from_url;
use swoop_engine::EngineEvent;
use swoop_store::{Store, downloads};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};
use url::Url;

/// Concluídos e falhos desde o último "fila terminou".
#[derive(Default)]
struct Tally {
    completed: u32,
    failed: u32,
}

/// Laço da tarefa: roda até o motor fechar o canal (ou o serviço abortar).
pub async fn forward(
    mut rx: broadcast::Receiver<EngineEvent>,
    store: Store,
    tx: broadcast::Sender<Push>,
) {
    let mut tally = Tally::default();
    loop {
        let mut batch = match rx.recv().await {
            Ok(ev) => vec![ev],
            // Eventos perdidos: a lista é recarregada; a contagem pode ficar curta.
            Err(RecvError::Lagged(_)) => Vec::new(),
            Err(RecvError::Closed) => return,
        };
        loop {
            match rx.try_recv() {
                Ok(ev) => batch.push(ev),
                Err(TryRecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
        let _ = tx.send(Push::Changed);

        let mut ended = false;
        for ev in batch {
            match ev {
                EngineEvent::Completed(_) => {
                    tally.completed += 1;
                    ended = true;
                }
                EngineEvent::Failed(id, message) => {
                    tally.failed += 1;
                    ended = true;
                    let name = display_name(&store, id).await;
                    let _ = tx.send(Push::Notice(Notice::Failed { name, message }));
                }
                _ => {}
            }
        }
        if ended && queue_idle(&store).await {
            let done = std::mem::take(&mut tally);
            let _ = tx.send(Push::Notice(Notice::QueueFinished {
                completed: done.completed,
                failed: done.failed,
            }));
        }
    }
}

/// Nada mais baixando nem esperando (pausados não contam).
async fn queue_idle(store: &Store) -> bool {
    match store.call(|c| downloads::list(c)).await {
        Ok(rows) => crate::pending_in(&rows) == 0,
        Err(e) => {
            tracing::warn!("não consegui conferir a fila: {e}");
            false
        }
    }
}

/// Nome do arquivo para o aviso (sem o link inteiro, que pode ter chave).
async fn display_name(store: &Store, id: DownloadId) -> String {
    let row = store
        .call(move |c| downloads::get(c, id))
        .await
        .ok()
        .flatten();
    row.and_then(|r| {
        r.file_name
            .or_else(|| Url::parse(&r.url).ok().and_then(|u| file_name_from_url(&u)))
    })
    .unwrap_or_else(|| format!("download {id}"))
}
