//! Thread dona da conexão SQLite. Serializa todos os acessos: sem disputa de
//! escritor, sem pool, e transações curtas.

use crate::StoreError;
use rusqlite::Connection;
use std::path::Path;
use std::sync::mpsc;
use std::thread;

type Job = Box<dyn FnOnce(&mut Connection) + Send>;

/// Handle clonável para a thread do banco. A thread termina quando o último
/// handle é descartado.
#[derive(Clone)]
pub struct Store {
    tx: mpsc::Sender<Job>,
}

impl Store {
    /// Abre (ou cria) o banco em `path`, aplica PRAGMAs e migrações.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        Self::start(conn)
    }

    /// Banco em memória (testes).
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::start(Connection::open_in_memory()?)
    }

    /// Configura a conexão e sobe a thread do ator.
    fn start(mut conn: Connection) -> Result<Self, StoreError> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        crate::migrations::run(&mut conn)?;

        let (tx, rx) = mpsc::channel::<Job>();
        thread::Builder::new()
            .name("swoop-store".into())
            .spawn(move || {
                for job in rx {
                    job(&mut conn);
                }
            })
            .map_err(|e| StoreError::Migration(format!("thread do banco: {e}")))?;
        Ok(Self { tx })
    }

    /// Executa `f` na thread do banco e aguarda o resultado (contexto async).
    pub async fn call<R, F>(&self, f: F) -> Result<R, StoreError>
    where
        R: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<R, StoreError> + Send + 'static,
    {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.send(f, reply_tx)?;
        reply_rx.await.map_err(|_| StoreError::Closed)?
    }

    /// Igual a `call`, para threads que não são do tokio (bloqueia).
    pub fn call_blocking<R, F>(&self, f: F) -> Result<R, StoreError>
    where
        R: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<R, StoreError> + Send + 'static,
    {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.send(f, reply_tx)?;
        reply_rx.blocking_recv().map_err(|_| StoreError::Closed)?
    }

    /// Enfileira o job com o canal de resposta.
    fn send<R, F>(
        &self,
        f: F,
        reply: tokio::sync::oneshot::Sender<Result<R, StoreError>>,
    ) -> Result<(), StoreError>
    where
        R: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<R, StoreError> + Send + 'static,
    {
        let job: Job = Box::new(move |conn| {
            let _ = reply.send(f(conn));
        });
        self.tx.send(job).map_err(|_| StoreError::Closed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn call_devolve_o_resultado() {
        let store = Store::open_in_memory().unwrap();
        let n: i64 = store
            .call(|c| Ok(c.query_row("SELECT 40 + 2", [], |r| r.get(0))?))
            .await
            .unwrap();
        assert_eq!(n, 42);
    }

    #[test]
    fn arquivo_em_disco_usa_wal() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("t.sqlite")).unwrap();
        let mode: String = store
            .call_blocking(|c| Ok(c.query_row("PRAGMA journal_mode", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(mode, "wal");
    }
}
