//! Persistência do Swoop.
//!
//! Uma única conexão SQLite vive numa thread própria (ator); o resto do
//! programa manda closures para ela via `Store::call` (async) ou
//! `Store::call_blocking` (threads comuns, como a escritora do motor).
//! Os repositórios são funções sobre `&Connection`, testáveis direto.

pub mod actor;
pub mod captcha;
pub mod collector;
pub mod downloads;
pub mod history;
pub mod host_state;
mod migrations;
pub mod packages;
pub mod segments;
pub mod settings;

pub use actor::Store;
pub use downloads::DownloadRow;
pub use rusqlite::Connection;
pub use segments::SegmentRow;

use swoop_core::{DownloadId, InvalidTransition};

/// Erro de persistência.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("erro do banco: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("falha ao migrar o banco: {0}")]
    Migration(String),
    #[error("o banco foi fechado")]
    Closed,
    #[error("download {0} não existe")]
    NotFound(DownloadId),
    #[error(transparent)]
    Transition(#[from] InvalidTransition),
    #[error("valor inválido no banco: {0}")]
    Corrupt(String),
}

/// Agora em milissegundos Unix (formato de tempo do banco).
pub fn now_ms() -> i64 {
    to_ms(std::time::SystemTime::now())
}

/// Converte `SystemTime` para milissegundos Unix.
pub fn to_ms(t: std::time::SystemTime) -> i64 {
    let d = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    i64::try_from(d.as_millis()).unwrap_or(i64::MAX)
}

/// Converte milissegundos Unix para `SystemTime`.
pub fn from_ms(ms: i64) -> std::time::SystemTime {
    std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms.max(0) as u64)
}
