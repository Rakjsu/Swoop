//! Fila de downloads. Toda mudança de estado passa por `transition`, que lê o
//! estado atual e aplica `DownloadState::next` na mesma chamada do ator: não
//! há como gravar uma transição inválida.

use crate::{StoreError, now_ms};
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::path::{Path, PathBuf};
use swoop_core::{DownloadId, DownloadState, Event, PackageId, WaitReason};

/// Linha da tabela `downloads`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadRow {
    pub id: DownloadId,
    pub package_id: PackageId,
    pub url: String,
    pub host_key: Option<String>,
    pub state: DownloadState,
    pub wait_until: Option<i64>,
    pub wait_reason: Option<WaitReason>,
    pub error_kind: Option<String>,
    pub error_msg: Option<String>,
    pub attempts: u32,
    pub file_name: Option<String>,
    pub size: Option<u64>,
    pub done_bytes: u64,
    pub resumable: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub part_path: Option<PathBuf>,
    pub final_path: Option<PathBuf>,
    pub position: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

/// O que a sonda descobriu sobre o arquivo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeInfo {
    pub host_key: String,
    pub file_name: String,
    pub size: Option<u64>,
    pub resumable: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub part_path: PathBuf,
}

const COLUMNS: &str = "id, package_id, url, host_key, state, wait_until, wait_reason, error_kind,
    error_msg, attempts, file_name, size, done_bytes, resumable, etag, last_modified, part_path,
    final_path, position, started_at, finished_at";

/// Converte uma linha do SELECT padrão.
fn from_row(r: &Row<'_>) -> rusqlite::Result<DownloadRow> {
    let state: String = r.get(4)?;
    let wait_reason: Option<String> = r.get(6)?;
    Ok(DownloadRow {
        id: DownloadId(r.get(0)?),
        package_id: PackageId(r.get(1)?),
        url: r.get(2)?,
        host_key: r.get(3)?,
        state: DownloadState::parse(&state).unwrap_or(DownloadState::Failed),
        wait_until: r.get(5)?,
        wait_reason: wait_reason.as_deref().and_then(WaitReason::parse),
        error_kind: r.get(7)?,
        error_msg: r.get(8)?,
        attempts: r.get(9)?,
        file_name: r.get(10)?,
        size: r.get::<_, Option<i64>>(11)?.map(|v| v as u64),
        done_bytes: r.get::<_, i64>(12)? as u64,
        resumable: r.get(13)?,
        etag: r.get(14)?,
        last_modified: r.get(15)?,
        part_path: r.get::<_, Option<String>>(16)?.map(PathBuf::from),
        final_path: r.get::<_, Option<String>>(17)?.map(PathBuf::from),
        position: r.get(18)?,
        started_at: r.get(19)?,
        finished_at: r.get(20)?,
    })
}

/// Adiciona um link no fim da fila, já como `queued`.
pub fn insert(c: &Connection, package: PackageId, url: &str) -> Result<DownloadId, StoreError> {
    c.execute(
        "INSERT INTO downloads (package_id, url, state, position, created_at)
         VALUES (?1, ?2, ?3, (SELECT COALESCE(MAX(position), 0) + 1 FROM downloads), ?4)",
        params![package.0, url, DownloadState::Queued.as_str(), now_ms()],
    )?;
    Ok(DownloadId(c.last_insert_rowid()))
}

/// Busca um download.
pub fn get(c: &Connection, id: DownloadId) -> Result<Option<DownloadRow>, StoreError> {
    let sql = format!("SELECT {COLUMNS} FROM downloads WHERE id = ?1");
    Ok(c.query_row(&sql, [id.0], from_row).optional()?)
}

/// Todos os downloads na ordem da fila.
pub fn list(c: &Connection) -> Result<Vec<DownloadRow>, StoreError> {
    let sql = format!("SELECT {COLUMNS} FROM downloads ORDER BY position");
    let mut stmt = c.prepare(&sql)?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Próximos da fila (estado `queued`), na ordem.
pub fn queued(c: &Connection, limit: usize) -> Result<Vec<DownloadRow>, StoreError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM downloads WHERE state = 'queued' ORDER BY position LIMIT ?1"
    );
    let mut stmt = c.prepare(&sql)?;
    let rows = stmt.query_map([limit as i64], from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Aplica um evento ao download e grava o novo estado (com efeitos colaterais
/// simples: carimbos de tempo, limpeza de erro e de espera).
pub fn transition(
    c: &Connection,
    id: DownloadId,
    event: Event,
) -> Result<DownloadState, StoreError> {
    let current: String = c
        .query_row("SELECT state FROM downloads WHERE id = ?1", [id.0], |r| {
            r.get(0)
        })
        .optional()?
        .ok_or(StoreError::NotFound(id))?;
    let from = DownloadState::parse(&current)
        .ok_or_else(|| StoreError::Corrupt(format!("estado {current:?}")))?;
    let to = from.next(event)?;

    let now = now_ms();
    c.execute(
        "UPDATE downloads SET state = ?2 WHERE id = ?1",
        params![id.0, to.as_str()],
    )?;
    if event == Event::Start {
        c.execute(
            "UPDATE downloads SET started_at = COALESCE(started_at, ?2) WHERE id = ?1",
            params![id.0, now],
        )?;
    }
    if matches!(event, Event::Retry | Event::Resume | Event::Resolved) {
        c.execute(
            "UPDATE downloads SET error_kind = NULL, error_msg = NULL WHERE id = ?1",
            [id.0],
        )?;
    }
    if matches!(event, Event::WaitElapsed | Event::Pause | Event::Retry) {
        c.execute(
            "UPDATE downloads SET wait_until = NULL, wait_reason = NULL WHERE id = ?1",
            [id.0],
        )?;
    }
    if to.is_final() {
        c.execute(
            "UPDATE downloads SET finished_at = ?2 WHERE id = ?1",
            params![id.0, now],
        )?;
    }
    Ok(to)
}

/// Coloca o download em espera até `until_ms`.
pub fn wait(
    c: &Connection,
    id: DownloadId,
    until_ms: i64,
    reason: WaitReason,
    message: Option<&str>,
) -> Result<(), StoreError> {
    transition(c, id, Event::Wait)?;
    c.execute(
        "UPDATE downloads SET wait_until = ?2, wait_reason = ?3, error_msg = ?4 WHERE id = ?1",
        params![id.0, until_ms, reason.as_str(), message],
    )?;
    Ok(())
}

/// Marca falha (`Event::Fail` ou `Event::IntegrityFailed`) com o motivo.
pub fn fail(
    c: &Connection,
    id: DownloadId,
    event: Event,
    kind: &str,
    message: &str,
) -> Result<(), StoreError> {
    transition(c, id, event)?;
    c.execute(
        "UPDATE downloads SET error_kind = ?2, error_msg = ?3 WHERE id = ?1",
        params![id.0, kind, message],
    )?;
    Ok(())
}

/// Esperas vencidas voltam para a fila. Devolve quantas.
pub fn wake_due(c: &Connection, now: i64) -> Result<usize, StoreError> {
    let ids: Vec<i64> = {
        let mut stmt = c.prepare(
            "SELECT id FROM downloads WHERE state = 'waiting' AND COALESCE(wait_until, 0) <= ?1",
        )?;
        stmt.query_map([now], |r| r.get(0))?
            .collect::<Result<_, _>>()?
    };
    for id in &ids {
        transition(c, DownloadId(*id), Event::WaitElapsed)?;
    }
    Ok(ids.len())
}

/// Ao abrir: o que estava em andamento volta para a fila. Devolve quantos.
pub fn recover_all(c: &Connection) -> Result<usize, StoreError> {
    Ok(c.execute(
        "UPDATE downloads SET state = 'queued'
         WHERE state IN ('resolving', 'downloading', 'verifying')",
        [],
    )?)
}

/// Grava o que a sonda descobriu (nome, tamanho, validadores, `.part`).
pub fn set_probe(c: &Connection, id: DownloadId, p: &ProbeInfo) -> Result<(), StoreError> {
    c.execute(
        "UPDATE downloads SET host_key = ?2, file_name = ?3, size = ?4, resumable = ?5,
                etag = ?6, last_modified = ?7, part_path = ?8 WHERE id = ?1",
        params![
            id.0,
            p.host_key,
            p.file_name,
            p.size.map(|s| s as i64),
            p.resumable,
            p.etag,
            p.last_modified,
            p.part_path.to_string_lossy(),
        ],
    )?;
    Ok(())
}

/// Caminho definitivo depois de renomear o `.part`.
pub fn set_final_path(c: &Connection, id: DownloadId, path: &Path) -> Result<(), StoreError> {
    c.execute(
        "UPDATE downloads SET final_path = ?2, part_path = NULL WHERE id = ?1",
        params![id.0, path.to_string_lossy()],
    )?;
    Ok(())
}

/// Tamanho final (downloads sem tamanho conhecido de antemão).
pub fn set_size(c: &Connection, id: DownloadId, size: u64) -> Result<(), StoreError> {
    c.execute(
        "UPDATE downloads SET size = ?2, done_bytes = ?2 WHERE id = ?1",
        params![id.0, size as i64],
    )?;
    Ok(())
}

/// Tentativas seguidas sem progresso (para o limite de retentativas).
pub fn set_attempts(c: &Connection, id: DownloadId, attempts: u32) -> Result<(), StoreError> {
    c.execute(
        "UPDATE downloads SET attempts = ?2 WHERE id = ?1",
        params![id.0, attempts],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "downloads_tests.rs"]
mod tests;
