//! Pacotes: grupos de links que vão para a mesma pasta.

use crate::{StoreError, now_ms};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::PathBuf;
use swoop_core::PackageId;

/// Linha da tabela `packages`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageRow {
    pub id: PackageId,
    pub name: String,
    pub dest_dir: PathBuf,
    /// Pasta escolhida por tipo de arquivo (`dest_dir` é o recuo).
    pub auto_dest: bool,
}

/// Cria um pacote no fim da lista. Com `auto_dest`, cada arquivo vai para a
/// pasta do seu tipo e `dest_dir` é só o recuo.
pub fn insert(
    c: &Connection,
    name: &str,
    dest_dir: &std::path::Path,
    auto_dest: bool,
) -> Result<PackageId, StoreError> {
    let dest = dest_dir.to_string_lossy();
    c.execute(
        "INSERT INTO packages (name, dest_dir, auto_dest, position, created_at)
         VALUES (?1, ?2, ?3, (SELECT COALESCE(MAX(position), 0) + 1 FROM packages), ?4)",
        params![name, dest, auto_dest, now_ms()],
    )?;
    Ok(PackageId(c.last_insert_rowid()))
}

/// Busca um pacote pelo id.
pub fn get(c: &Connection, id: PackageId) -> Result<Option<PackageRow>, StoreError> {
    Ok(c.query_row(
        "SELECT id, name, dest_dir, auto_dest FROM packages WHERE id = ?1",
        [id.0],
        |r| {
            Ok(PackageRow {
                id: PackageId(r.get(0)?),
                name: r.get(1)?,
                dest_dir: PathBuf::from(r.get::<_, String>(2)?),
                auto_dest: r.get(3)?,
            })
        },
    )
    .optional()?)
}
