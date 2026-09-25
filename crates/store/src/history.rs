//! Histórico: o que terminou (concluído, falhou, removido). Só inserções.

use crate::{StoreError, now_ms};
use rusqlite::{Connection, params};
use swoop_core::DownloadId;

/// Resultado registrado no histórico.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Completed,
    Failed,
    Removed,
}

impl Outcome {
    /// Nome estável usado no banco.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Removed => "removed",
        }
    }
}

/// Copia os dados atuais do download para o histórico, com a velocidade média.
pub fn record(c: &Connection, id: DownloadId, outcome: Outcome) -> Result<(), StoreError> {
    let now = now_ms();
    c.execute(
        "INSERT INTO history (url, host_key, file_name, size, final_path, outcome, error_msg,
                              started_at, finished_at, avg_bps)
         SELECT url, host_key, file_name, size, final_path, ?2, error_msg, started_at, ?3,
                CASE WHEN started_at IS NOT NULL AND ?3 > started_at
                     THEN done_bytes * 1000 / (?3 - started_at) END
         FROM downloads WHERE id = ?1",
        params![id.0, outcome.as_str(), now],
    )?;
    Ok(())
}

/// Quantas entradas existem (usado em testes e na tela de histórico).
pub fn count(c: &Connection) -> Result<u64, StoreError> {
    Ok(c.query_row("SELECT COUNT(*) FROM history", [], |r| r.get::<_, i64>(0))? as u64)
}
