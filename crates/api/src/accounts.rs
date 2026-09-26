//! Contas premium vistas pela interface: nunca levam o segredo (senha ou
//! chave), que só vai do formulário para o cofre do sistema.

use serde::Serialize;
use swoop_core::{AccountKind, AccountStatus};
use ts_rs::TS;

/// Uma conta cadastrada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct AccountView {
    /// Servidor (`fastfile.cc`).
    pub host: String,
    /// Usuário (vazio em conta por chave de API).
    pub username: String,
    pub kind: AccountKind,
    pub status: AccountStatus,
    /// Fim do premium (ms Unix), se o site informa.
    pub premium_until_ms: Option<i64>,
    /// Tráfego restante (bytes), se o site informa.
    pub traffic_left: Option<u64>,
    /// Última conferência (ms Unix).
    pub checked_ms: Option<i64>,
    /// Motivo da recusa ou da falha na conferência.
    pub error: Option<String>,
}

/// Aba Contas: servidores que aceitam conta e as contas cadastradas.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct AccountsView {
    pub hosts: Vec<String>,
    pub accounts: Vec<AccountView>,
}
