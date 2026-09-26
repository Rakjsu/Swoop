//! Contas em uso, por servidor (`host_key`). O serviço carrega do banco + cofre
//! e mantém esta cópia em memória; os plugins só leem.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use swoop_core::Account;

/// Contas por servidor, compartilhadas entre o serviço e os plugins.
#[derive(Clone, Default)]
pub struct Accounts(Arc<RwLock<HashMap<String, Account>>>);

impl Accounts {
    /// Conta do servidor, se o usuário cadastrou uma.
    pub fn get(&self, host_key: &str) -> Option<Account> {
        self.0.read().expect("contas").get(host_key).cloned()
    }

    pub fn set(&self, host_key: &str, account: Account) {
        self.0
            .write()
            .expect("contas")
            .insert(host_key.to_owned(), account);
    }

    pub fn remove(&self, host_key: &str) {
        self.0.write().expect("contas").remove(host_key);
    }
}
