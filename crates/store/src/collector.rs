//! Coletor: links colados esperando verificação e o "Iniciar". A mesma URL
//! não entra duas vezes; `batch` junta os links de uma mesma colagem.

use crate::{StoreError, now_ms};
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::collections::HashSet;
use swoop_core::LinkState;

/// Linha da tabela `collector`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectedRow {
    pub id: i64,
    pub url: String,
    /// Servidor (plugin ou domínio): limita verificações simultâneas.
    pub host_key: String,
    pub batch: i64,
    pub state: LinkState,
    pub file_name: Option<String>,
    pub size: Option<u64>,
    pub error: Option<String>,
}

const COLUMNS: &str = "id, url, host_key, batch, state, file_name, size, error";

fn from_row(r: &Row<'_>) -> rusqlite::Result<CollectedRow> {
    let state: String = r.get(4)?;
    Ok(CollectedRow {
        id: r.get(0)?,
        url: r.get(1)?,
        host_key: r.get(2)?,
        batch: r.get(3)?,
        state: LinkState::parse(&state).unwrap_or(LinkState::Unchecked),
        file_name: r.get(5)?,
        size: r.get::<_, Option<i64>>(6)?.map(|s| s as u64),
        error: r.get(7)?,
    })
}

/// Número da próxima colagem.
pub fn next_batch(c: &Connection) -> Result<i64, StoreError> {
    Ok(c.query_row(
        "SELECT COALESCE(MAX(batch), 0) + 1 FROM collector",
        [],
        |r| r.get(0),
    )?)
}

/// Guarda um link; `false` quando a URL já estava no coletor.
pub fn insert(c: &Connection, url: &str, host_key: &str, batch: i64) -> Result<bool, StoreError> {
    let n = c.execute(
        "INSERT OR IGNORE INTO collector (url, host_key, batch, added_at) VALUES (?1, ?2, ?3, ?4)",
        params![url, host_key, batch, now_ms()],
    )?;
    Ok(n == 1)
}

/// Todos os links, na ordem em que foram colados.
pub fn list(c: &Connection) -> Result<Vec<CollectedRow>, StoreError> {
    let mut stmt = c.prepare(&format!("SELECT {COLUMNS} FROM collector ORDER BY id"))?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Pega até `max` links não verificados, no máximo um por servidor, e os
/// marca como "verificando".
pub fn claim(c: &Connection, max: usize) -> Result<Vec<CollectedRow>, StoreError> {
    let mut stmt = c.prepare(&format!(
        "SELECT {COLUMNS} FROM collector WHERE state = 'unchecked' ORDER BY id"
    ))?;
    let pending: Vec<CollectedRow> = stmt.query_map([], from_row)?.collect::<Result<_, _>>()?;
    let busy: HashSet<String> = {
        let mut s =
            c.prepare("SELECT DISTINCT host_key FROM collector WHERE state = 'checking'")?;
        s.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?
    };
    let mut hosts = busy;
    let mut out = Vec::new();
    for mut row in pending {
        if out.len() >= max {
            break;
        }
        if hosts.insert(row.host_key.clone()) {
            c.execute(
                "UPDATE collector SET state = 'checking' WHERE id = ?1",
                [row.id],
            )?;
            row.state = LinkState::Checking;
            out.push(row);
        }
    }
    Ok(out)
}

/// Grava o resultado da verificação.
pub fn set_result(
    c: &Connection,
    id: i64,
    state: LinkState,
    file_name: Option<&str>,
    size: Option<u64>,
    error: Option<&str>,
) -> Result<(), StoreError> {
    c.execute(
        "UPDATE collector SET state = ?2, file_name = COALESCE(?3, file_name),
                size = COALESCE(?4, size), error = ?5 WHERE id = ?1",
        params![id, state.as_str(), file_name, size.map(|s| s as i64), error],
    )?;
    Ok(())
}

/// A colagem ainda tem link por verificar?
pub fn batch_pending(c: &Connection, batch: i64) -> Result<bool, StoreError> {
    Ok(c.query_row(
        "SELECT EXISTS(SELECT 1 FROM collector WHERE batch = ?1
                       AND state IN ('unchecked', 'checking'))",
        [batch],
        |r| r.get(0),
    )?)
}

/// Ids dos links online de uma colagem.
pub fn online_in_batch(c: &Connection, batch: i64) -> Result<Vec<i64>, StoreError> {
    let mut stmt =
        c.prepare("SELECT id FROM collector WHERE batch = ?1 AND state = 'online' ORDER BY id")?;
    let ids = stmt.query_map([batch], |r| r.get(0))?;
    Ok(ids.collect::<Result<_, _>>()?)
}

/// Tira do coletor os links pedidos que podem baixar (todos menos offline e
/// os que ainda estão sendo verificados) e os devolve na ordem.
pub fn take(c: &Connection, ids: &[i64]) -> Result<Vec<CollectedRow>, StoreError> {
    let mut stmt = c.prepare(&format!(
        "SELECT {COLUMNS} FROM collector WHERE id = ?1 AND state NOT IN ('offline', 'checking')"
    ))?;
    let mut out = Vec::new();
    for id in ids {
        if let Some(row) = stmt.query_row([id], from_row).optional()? {
            c.execute("DELETE FROM collector WHERE id = ?1", [id])?;
            out.push(row);
        }
    }
    out.sort_by_key(|r| r.id);
    Ok(out)
}

/// Remove os links pedidos; devolve quantos saíram.
pub fn remove(c: &Connection, ids: &[i64]) -> Result<usize, StoreError> {
    let mut n = 0;
    for id in ids {
        n += c.execute("DELETE FROM collector WHERE id = ?1", [id])?;
    }
    Ok(n)
}

/// Remove todos os offline.
pub fn remove_offline(c: &Connection) -> Result<usize, StoreError> {
    Ok(c.execute("DELETE FROM collector WHERE state = 'offline'", [])?)
}

/// Esvazia o coletor.
pub fn clear(c: &Connection) -> Result<usize, StoreError> {
    Ok(c.execute("DELETE FROM collector", [])?)
}

/// Verificações interrompidas (app fechado no meio) voltam para a fila.
pub fn recover(c: &Connection) -> Result<usize, StoreError> {
    Ok(c.execute(
        "UPDATE collector SET state = 'unchecked' WHERE state = 'checking'",
        [],
    )?)
}

#[cfg(test)]
#[path = "collector_tests.rs"]
mod tests;
