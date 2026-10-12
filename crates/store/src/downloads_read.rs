//! Leituras da tabela `downloads`: por id, a lista, a fila (em páginas) e
//! por estado; e a chave do servidor de uma linha.

use super::{COLUMNS, DownloadRow, from_row};
use crate::StoreError;
use rusqlite::{Connection, OptionalExtension, params};
use swoop_core::{DownloadId, DownloadState};

impl DownloadRow {
    /// Chave do servidor: a gravada (plugin, ao adicionar ou resolver) ou o
    /// domínio do link sem `www.`.
    pub fn server(&self) -> String {
        self.host_key.clone().unwrap_or_else(|| self.url_server())
    }

    /// Domínio do link sem `www.` (esperas gravadas antes da chave existir).
    pub fn url_server(&self) -> String {
        url::Url::parse(&self.url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
            .map(|h| h.trim_start_matches("www.").to_owned())
            .unwrap_or_default()
    }
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
    queued_after(c, i64::MIN, limit)
}

/// Uma página da fila: os `queued` depois da posição `after`, na ordem (o
/// agendador lê página a página, sem janela fixa).
pub fn queued_after(
    c: &Connection,
    after: i64,
    limit: usize,
) -> Result<Vec<DownloadRow>, StoreError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM downloads WHERE state = 'queued' AND position > ?1
         ORDER BY position LIMIT ?2"
    );
    let mut stmt = c.prepare(&sql)?;
    let rows = stmt.query_map(params![after, limit as i64], from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Na fila com a resposta do captcha guardada (o envio tem prazo): passam na
/// frente dos outros.
pub fn queued_answered(c: &Connection) -> Result<Vec<DownloadRow>, StoreError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM downloads
         WHERE state = 'queued' AND captcha IS NOT NULL AND captcha_token IS NOT NULL
         ORDER BY position"
    );
    let mut stmt = c.prepare(&sql)?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Downloads num estado (ex.: os que esperam captcha), na ordem da fila.
pub fn in_state(c: &Connection, state: DownloadState) -> Result<Vec<DownloadRow>, StoreError> {
    let sql = format!("SELECT {COLUMNS} FROM downloads WHERE state = ?1 ORDER BY position");
    let mut stmt = c.prepare(&sql)?;
    let rows = stmt.query_map([state.as_str()], from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}
