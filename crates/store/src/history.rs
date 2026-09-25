//! Histórico: o que terminou (concluído, falhou, removido). Só inserções.

use crate::{StoreError, now_ms};
use rusqlite::{Connection, params};
use std::path::PathBuf;
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

/// Uma entrada do histórico.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRow {
    pub id: i64,
    pub url: String,
    pub file_name: Option<String>,
    pub size: Option<u64>,
    pub final_path: Option<PathBuf>,
    /// `completed`, `failed` ou `removed` (ver `Outcome::as_str`).
    pub outcome: String,
    pub error_msg: Option<String>,
    pub finished_at: i64,
    pub avg_bps: Option<u64>,
}

/// As `limit` entradas mais recentes, da mais nova para a mais velha.
pub fn list(c: &Connection, limit: u32) -> Result<Vec<HistoryRow>, StoreError> {
    let mut stmt = c.prepare(
        "SELECT id, url, file_name, size, final_path, outcome, error_msg, finished_at, avg_bps
         FROM history ORDER BY finished_at DESC, id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |r| {
        Ok(HistoryRow {
            id: r.get(0)?,
            url: r.get(1)?,
            file_name: r.get(2)?,
            size: r.get::<_, Option<i64>>(3)?.map(|v| v as u64),
            final_path: r.get::<_, Option<String>>(4)?.map(PathBuf::from),
            outcome: r.get(5)?,
            error_msg: r.get(6)?,
            finished_at: r.get(7)?,
            avg_bps: r.get::<_, Option<i64>>(8)?.map(|v| v as u64),
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Quantas entradas existem (usado em testes e na tela de histórico).
pub fn count(c: &Connection) -> Result<u64, StoreError> {
    Ok(c.query_row("SELECT COUNT(*) FROM history", [], |r| r.get::<_, i64>(0))? as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{downloads, packages};
    use std::path::Path;

    #[test]
    fn lista_do_mais_novo_para_o_mais_velho() {
        let mut c = Connection::open_in_memory().unwrap();
        crate::migrations::run(&mut c).unwrap();
        let pkg = packages::insert(&c, "p", Path::new("/tmp"), false).unwrap();
        let a = downloads::insert(&c, pkg, "http://x/a").unwrap();
        let b = downloads::insert(&c, pkg, "http://x/b").unwrap();
        record(&c, a, Outcome::Completed).unwrap();
        record(&c, b, Outcome::Removed).unwrap();
        let rows = list(&c, 10).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].url, "http://x/b");
        assert_eq!(rows[0].outcome, "removed");
        assert_eq!(list(&c, 1).unwrap().len(), 1);
    }
}
