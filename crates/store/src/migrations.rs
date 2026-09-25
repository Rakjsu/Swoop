//! Migrações do esquema, aplicadas em ordem ao abrir o banco.
//!
//! A versão fica em `PRAGMA user_version`: a migração N (1-based) roda se a
//! versão atual for menor que N, cada uma na sua transação. Migração nova =
//! arquivo novo no fim da lista; nunca editar uma que já foi publicada.

use crate::StoreError;
use rusqlite::Connection;

const MIGRATIONS: &[&str] = &[
    include_str!("migrations/0001_init.sql"),
    include_str!("migrations/0002_settings.sql"),
    include_str!("migrations/0003_collector.sql"),
];

/// Leva o banco à última versão do esquema.
pub(crate) fn run(conn: &mut Connection) -> Result<(), StoreError> {
    let current = conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?;
    let current = usize::try_from(current).unwrap_or(0);
    if current > MIGRATIONS.len() {
        return Err(StoreError::Migration(format!(
            "o banco está na versão {current}, mais nova que este Swoop ({}); atualize o app",
            MIGRATIONS.len()
        )));
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)
            .map_err(|e| StoreError::Migration(format!("migração {}: {e}", i + 1)))?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aplica_todas_e_e_idempotente() {
        let mut c = Connection::open_in_memory().unwrap();
        run(&mut c).unwrap();
        run(&mut c).unwrap();
        let v: i64 = c
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v as usize, MIGRATIONS.len());
    }

    #[test]
    fn recusa_banco_de_versao_futura() {
        let mut c = Connection::open_in_memory().unwrap();
        c.pragma_update(None, "user_version", 999).unwrap();
        assert!(matches!(run(&mut c), Err(StoreError::Migration(_))));
    }
}
