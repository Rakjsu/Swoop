//! Contas premium, uma por servidor. Nenhum segredo aqui: só `secret_ref`, o
//! nome da entrada no cofre do sistema.

use crate::StoreError;
use rusqlite::{Connection, OptionalExtension, Row, params};
use swoop_core::{AccountInfo, AccountKind, AccountStatus};

/// Uma conta como está no banco.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRow {
    pub host_key: String,
    pub username: String,
    pub kind: AccountKind,
    /// Entrada no cofre do sistema com a senha/chave.
    pub secret_ref: String,
    pub status: AccountStatus,
    pub premium_until: Option<i64>,
    pub traffic_left: Option<u64>,
    /// Última conferência (ms Unix).
    pub checked_at: Option<i64>,
    pub error: Option<String>,
}

const COLUMNS: &str = "host_key, username, kind, secret_ref, status, premium_until, \
                       traffic_left, checked_at, error";

fn from_row(r: &Row<'_>) -> rusqlite::Result<AccountRow> {
    let kind: String = r.get(2)?;
    let status: String = r.get(4)?;
    Ok(AccountRow {
        host_key: r.get(0)?,
        username: r.get(1)?,
        kind: AccountKind::parse(&kind).unwrap_or(AccountKind::Login),
        secret_ref: r.get(3)?,
        status: AccountStatus::parse(&status),
        premium_until: r.get(5)?,
        traffic_left: r.get::<_, Option<i64>>(6)?.map(|t| t.max(0) as u64),
        checked_at: r.get(7)?,
        error: r.get(8)?,
    })
}

/// Grava a conta do servidor (troca a anterior, que volta como `secret_ref`
/// antigo para sair do cofre).
pub fn upsert(
    c: &Connection,
    host_key: &str,
    username: &str,
    kind: AccountKind,
    secret_ref: &str,
) -> Result<Option<String>, StoreError> {
    let old = get(c, host_key)?.map(|a| a.secret_ref);
    c.execute(
        "INSERT INTO accounts (host_key, username, kind, secret_ref, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(host_key) DO UPDATE SET
            username = excluded.username, kind = excluded.kind,
            secret_ref = excluded.secret_ref, status = 'unchecked',
            premium_until = NULL, traffic_left = NULL, checked_at = NULL, error = NULL",
        params![
            host_key,
            username,
            kind.as_str(),
            secret_ref,
            crate::now_ms()
        ],
    )?;
    Ok(old.filter(|o| o != secret_ref))
}

/// Conta do servidor, se houver.
pub fn get(c: &Connection, host_key: &str) -> Result<Option<AccountRow>, StoreError> {
    let sql = format!("SELECT {COLUMNS} FROM accounts WHERE host_key = ?1");
    Ok(c.query_row(&sql, [host_key], from_row).optional()?)
}

/// Todas as contas, por servidor.
pub fn list(c: &Connection) -> Result<Vec<AccountRow>, StoreError> {
    let sql = format!("SELECT {COLUMNS} FROM accounts ORDER BY host_key");
    let mut stmt = c.prepare(&sql)?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Resultado da conferência no site.
pub fn set_check(
    c: &Connection,
    host_key: &str,
    status: AccountStatus,
    info: &AccountInfo,
    error: Option<&str>,
) -> Result<(), StoreError> {
    c.execute(
        "UPDATE accounts SET status = ?2, premium_until = ?3, traffic_left = ?4,
            checked_at = ?5, error = ?6 WHERE host_key = ?1",
        params![
            host_key,
            status.as_str(),
            info.premium_until_ms,
            info.traffic_left.map(|t| t.min(i64::MAX as u64) as i64),
            crate::now_ms(),
            error
        ],
    )?;
    Ok(())
}

/// Tira a conta; devolve o `secret_ref` para apagar do cofre.
pub fn remove(c: &Connection, host_key: &str) -> Result<Option<String>, StoreError> {
    let old = get(c, host_key)?.map(|a| a.secret_ref);
    c.execute("DELETE FROM accounts WHERE host_key = ?1", [host_key])?;
    Ok(old)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conta_por_servidor_troca_confere_e_sai() {
        let mut c = Connection::open_in_memory().unwrap();
        crate::migrations::run(&mut c).unwrap();
        assert_eq!(
            upsert(&c, "fastfile.cc", "ana", AccountKind::Login, "ref-1").unwrap(),
            None
        );
        let info = AccountInfo {
            premium: true,
            premium_until_ms: Some(9_000),
            traffic_left: Some(1 << 40),
        };
        set_check(&c, "fastfile.cc", AccountStatus::Premium, &info, None).unwrap();
        let a = get(&c, "fastfile.cc").unwrap().unwrap();
        assert_eq!(a.status, AccountStatus::Premium);
        assert_eq!(a.traffic_left, Some(1 << 40));

        // Trocar a conta devolve o segredo antigo e zera a conferência.
        let old = upsert(&c, "fastfile.cc", "ana2", AccountKind::ApiKey, "ref-2").unwrap();
        assert_eq!(old.as_deref(), Some("ref-1"));
        let a = get(&c, "fastfile.cc").unwrap().unwrap();
        assert_eq!((a.username.as_str(), a.kind), ("ana2", AccountKind::ApiKey));
        assert_eq!(a.status, AccountStatus::Unchecked);
        assert_eq!(list(&c).unwrap().len(), 1);

        assert_eq!(remove(&c, "fastfile.cc").unwrap().as_deref(), Some("ref-2"));
        assert!(list(&c).unwrap().is_empty());
    }
}
