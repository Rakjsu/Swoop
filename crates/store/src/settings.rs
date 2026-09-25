//! Preferências guardadas como JSON por chave. O store só guarda texto; quem
//! chama (o serviço) serializa e valida.

use crate::StoreError;
use rusqlite::{Connection, OptionalExtension, params};

/// Valor salvo na chave, se houver.
pub fn load(c: &Connection, key: &str) -> Result<Option<String>, StoreError> {
    Ok(
        c.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?,
    )
}

/// Grava (ou troca) o valor da chave.
pub fn save(c: &Connection, key: &str, value: &str) -> Result<(), StoreError> {
    c.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grava_le_e_troca() {
        let mut c = Connection::open_in_memory().unwrap();
        crate::migrations::run(&mut c).unwrap();
        assert_eq!(load(&c, "engine").unwrap(), None);
        save(&c, "engine", "{\"a\":1}").unwrap();
        save(&c, "engine", "{\"a\":2}").unwrap();
        assert_eq!(load(&c, "engine").unwrap().as_deref(), Some("{\"a\":2}"));
    }
}
