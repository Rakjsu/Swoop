//! Contas em uso, por servidor (`host_key`). O serviço carrega do banco + cofre
//! e mantém esta cópia em memória; os plugins só leem. A sessão aberta com a
//! conta (token de API) também mora só aqui, nunca em disco.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use swoop_core::{Account, Secret};

/// Sessão de API aberta com a conta do usuário.
#[derive(Clone)]
pub struct Session {
    pub token: Secret,
    /// A conta era premium no login.
    pub premium: bool,
    pub opened: Instant,
}

#[derive(Default)]
struct Book {
    accounts: HashMap<String, Account>,
    sessions: HashMap<String, Session>,
    /// Conta recusada pelo site (senha errada, bloqueio): o motivo.
    refused: HashMap<String, String>,
}

/// Contas por servidor, compartilhadas entre o serviço e os plugins.
#[derive(Clone, Default)]
pub struct Accounts {
    book: Arc<RwLock<Book>>,
    /// Um login por vez (o site bloqueia logins repetidos).
    login: Arc<tokio::sync::Mutex<()>>,
}

impl Accounts {
    /// Conta do servidor, se o usuário cadastrou uma.
    pub fn get(&self, host_key: &str) -> Option<Account> {
        self.book
            .read()
            .expect("contas")
            .accounts
            .get(host_key)
            .cloned()
    }

    /// Cadastra ou troca a conta (a sessão da anterior deixa de valer).
    pub fn set(&self, host_key: &str, account: Account) {
        let mut b = self.book.write().expect("contas");
        b.accounts.insert(host_key.to_owned(), account);
        b.sessions.remove(host_key);
        b.refused.remove(host_key);
    }

    pub fn remove(&self, host_key: &str) {
        let mut b = self.book.write().expect("contas");
        b.accounts.remove(host_key);
        b.sessions.remove(host_key);
        b.refused.remove(host_key);
    }

    /// Lembra que o site recusou a conta (até ela ser trocada).
    pub fn refuse(&self, host_key: &str, username: &str, reason: &str) {
        let mut b = self.book.write().expect("contas");
        if b.accounts
            .get(host_key)
            .is_some_and(|a| a.username == username)
        {
            b.refused.insert(host_key.to_owned(), reason.to_owned());
            b.sessions.remove(host_key);
        }
    }

    /// Motivo da recusa da conta, se o site recusou.
    pub fn refused(&self, host_key: &str) -> Option<String> {
        self.book
            .read()
            .expect("contas")
            .refused
            .get(host_key)
            .cloned()
    }

    /// Sessão aberta com a conta do servidor.
    pub fn session(&self, host_key: &str) -> Option<Session> {
        self.book
            .read()
            .expect("contas")
            .sessions
            .get(host_key)
            .cloned()
    }

    /// Guarda a sessão, se a conta ainda for a mesma do login.
    pub fn set_session(&self, host_key: &str, username: &str, session: Session) {
        let mut b = self.book.write().expect("contas");
        if b.accounts
            .get(host_key)
            .is_some_and(|a| a.username == username)
        {
            b.sessions.insert(host_key.to_owned(), session);
        }
    }

    /// Trava de login (um por vez em todo o app).
    pub fn login_lock(&self) -> Arc<tokio::sync::Mutex<()>> {
        self.login.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swoop_core::AccountKind;

    fn account(user: &str) -> Account {
        Account {
            username: user.into(),
            kind: AccountKind::Login,
            secret: Secret::new("senha"),
        }
    }

    #[test]
    fn trocar_ou_remover_a_conta_derruba_a_sessao() {
        let a = Accounts::default();
        let session = || Session {
            token: Secret::new("tok"),
            premium: true,
            opened: Instant::now(),
        };
        // sessão de conta que não está mais cadastrada não entra
        a.set_session("rg", "x", session());
        assert!(a.session("rg").is_none());
        a.set("rg", account("x"));
        a.set_session("rg", "x", session());
        assert!(a.session("rg").is_some());
        a.set("rg", account("y"));
        assert!(a.session("rg").is_none());
        a.set_session("rg", "y", session());
        a.refuse("rg", "y", "senha errada");
        assert_eq!(a.refused("rg").as_deref(), Some("senha errada"));
        assert!(a.session("rg").is_none(), "recusa derruba a sessão");
        a.set("rg", account("y"));
        assert!(a.refused("rg").is_none(), "conta trocada tenta de novo");
        a.remove("rg");
        assert!(a.session("rg").is_none() && a.get("rg").is_none());
    }
}
