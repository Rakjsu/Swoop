//! Esperas por servidor: quando um servidor manda esperar antes do próximo
//! download, todos os downloads dele esperam, mesmo depois de reabrir o app.

use crate::StoreError;
use rusqlite::{Connection, params};
use swoop_core::WaitReason;

/// Espera ativa de um servidor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostWait {
    pub host_key: String,
    /// ms Unix.
    pub wait_until: i64,
    pub reason: WaitReason,
}

/// Grava (ou estende) a espera do servidor.
pub fn set_wait(
    c: &Connection,
    host_key: &str,
    wait_until: i64,
    reason: WaitReason,
) -> Result<(), StoreError> {
    c.execute(
        "INSERT INTO host_state (host_key, wait_until, reason) VALUES (?1, ?2, ?3)
         ON CONFLICT(host_key) DO UPDATE SET
            wait_until = MAX(wait_until, excluded.wait_until), reason = excluded.reason",
        params![host_key, wait_until, reason.as_str()],
    )?;
    Ok(())
}

/// Servidores ainda esperando em `now` (ms); as esperas vencidas saem.
pub fn active(c: &Connection, now: i64) -> Result<Vec<HostWait>, StoreError> {
    c.execute("DELETE FROM host_state WHERE wait_until <= ?1", [now])?;
    let mut stmt = c.prepare("SELECT host_key, wait_until, reason FROM host_state")?;
    let rows = stmt.query_map([], |r| {
        let reason: String = r.get(2)?;
        Ok(HostWait {
            host_key: r.get(0)?,
            wait_until: r.get(1)?,
            reason: WaitReason::parse(&reason).unwrap_or(WaitReason::HostLimit),
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn espera_estende_e_vence() {
        let mut c = Connection::open_in_memory().unwrap();
        crate::migrations::run(&mut c).unwrap();
        set_wait(&c, "fastfile", 5_000, WaitReason::HostLimit).unwrap();
        set_wait(&c, "fastfile", 3_000, WaitReason::HostLimit).unwrap(); // não encurta
        let waits = active(&c, 1_000).unwrap();
        assert_eq!(waits.len(), 1);
        assert_eq!(waits[0].wait_until, 5_000);
        assert!(active(&c, 5_000).unwrap().is_empty());
    }
}
