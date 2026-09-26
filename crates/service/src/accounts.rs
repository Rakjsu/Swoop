//! Contas premium: o segredo vai para o cofre do sistema, o banco guarda o
//! resto (e o nome da entrada no cofre), e os plugins leem a conta em memória
//! (`swoop_hosts::Accounts`). Uma conta por servidor.

use crate::vault::Vault;
use crate::{Service, ServiceError};
use swoop_api::{AccountView, Push};
use swoop_core::{Account, AccountInfo, AccountKind, AccountStatus, HostError, Secret};
use swoop_hosts::{Accounts, Registry};
use swoop_store::accounts::{self, AccountRow};
use swoop_store::{Store, now_ms};
use tokio::sync::broadcast;

/// Carrega as contas do banco + cofre para os plugins (na abertura). Conta
/// sem segredo no cofre fica marcada com erro e não é usada.
pub(crate) async fn load(
    store: &Store,
    vault: &dyn Vault,
    book: &Accounts,
) -> Result<(), ServiceError> {
    for row in store.call(|c| accounts::list(c)).await? {
        match vault.get(&row.secret_ref) {
            Ok(Some(secret)) => book.set(&row.host_key, account(&row, secret)),
            Ok(None) => {
                let host = row.host_key.clone();
                let msg = "a senha ou chave sumiu do cofre do sistema; cadastre a conta de novo";
                store
                    .call(move |c| {
                        accounts::set_check(
                            c,
                            &host,
                            AccountStatus::Error,
                            &AccountInfo::default(),
                            Some(msg),
                        )
                    })
                    .await?;
            }
            Err(e) => tracing::warn!(host = row.host_key, "conta não carregada: {e}"),
        }
    }
    Ok(())
}

/// Linha do banco → conta para a interface (sem segredo).
pub(crate) fn view(r: AccountRow) -> AccountView {
    AccountView {
        host: r.host_key,
        username: r.username,
        kind: r.kind,
        status: r.status,
        premium_until_ms: r.premium_until,
        traffic_left: r.traffic_left,
        checked_ms: r.checked_at,
        error: r.error,
    }
}

fn account(row: &AccountRow, secret: Secret) -> Account {
    Account {
        username: row.username.clone(),
        kind: row.kind,
        secret,
    }
}

/// "WWW.FastFile.cc " → "fastfile.cc".
fn normalize(host: &str) -> String {
    let host = host.trim().to_ascii_lowercase();
    host.strip_prefix("www.").unwrap_or(&host).to_owned()
}

impl Service {
    /// Cadastra (ou troca) a conta do servidor e confere em segundo plano.
    /// O segredo vai direto para o cofre; o banco nunca o vê.
    pub async fn add_account(
        &self,
        host: &str,
        username: &str,
        kind: AccountKind,
        secret: Secret,
    ) -> Result<(), ServiceError> {
        let host = normalize(host);
        let username = username.trim().to_owned();
        if !self.hosts.accepts_account(&host) {
            return Err(ServiceError::BadInput(format!(
                "o Swoop não usa conta em \"{host}\""
            )));
        }
        if secret.is_blank() {
            return Err(ServiceError::BadInput("informe a senha ou a chave".into()));
        }
        if kind == AccountKind::Login && username.is_empty() {
            return Err(ServiceError::BadInput("informe o usuário".into()));
        }
        let secret_ref = format!("{host}/{username}/{}", now_ms());
        self.vault.put(&secret_ref, &secret)?;
        let (h, u, r) = (host.clone(), username.clone(), secret_ref.clone());
        let old = match self
            .store
            .call(move |c| accounts::upsert(c, &h, &u, kind, &r))
            .await
        {
            Ok(old) => old,
            Err(e) => {
                let _ = self.vault.delete(&secret_ref);
                return Err(e.into());
            }
        };
        if let Some(old) = old
            && let Err(e) = self.vault.delete(&old)
        {
            tracing::warn!(host, "segredo antigo ficou no cofre: {e}");
        }
        let acc = Account {
            username,
            kind,
            secret,
        };
        self.hosts.accounts().set(&host, acc);
        let _ = self.pushes.send(Push::Changed);
        self.spawn_check(host);
        Ok(())
    }

    /// Tira a conta do servidor (do banco, do cofre e dos plugins).
    pub async fn remove_account(&self, host: &str) -> Result<(), ServiceError> {
        let host = normalize(host);
        let h = host.clone();
        let old = self.store.call(move |c| accounts::remove(c, &h)).await?;
        self.hosts.accounts().remove(&host);
        if let Some(old) = old {
            self.vault.delete(&old)?;
        }
        let _ = self.pushes.send(Push::Changed);
        Ok(())
    }

    /// Confere a conta no site e grava o resultado (premium, validade).
    pub async fn check_account(&self, host: &str) -> Result<AccountStatus, ServiceError> {
        check(&self.hosts, &self.store, &self.pushes, normalize(host)).await
    }

    /// Confere em segundo plano (a interface é avisada com `Changed`).
    pub(crate) fn spawn_check(&self, host: String) {
        let (hosts, store, pushes) = (self.hosts.clone(), self.store.clone(), self.pushes.clone());
        tokio::spawn(async move {
            if let Err(e) = check(&hosts, &store, &pushes, host.clone()).await {
                tracing::warn!(host, "não consegui conferir a conta: {e}");
            }
        });
    }

    /// Contas cadastradas (sem segredo).
    pub async fn account_rows(&self) -> Result<Vec<AccountRow>, ServiceError> {
        Ok(self.store.call(|c| accounts::list(c)).await?)
    }

    /// Servidores que aceitam conta.
    pub fn account_hosts(&self) -> Vec<String> {
        self.hosts.account_hosts()
    }
}

/// Pergunta ao site como está a conta e grava.
async fn check(
    hosts: &Registry,
    store: &Store,
    pushes: &broadcast::Sender<Push>,
    host: String,
) -> Result<AccountStatus, ServiceError> {
    let acc = hosts
        .accounts()
        .get(&host)
        .ok_or_else(|| ServiceError::BadInput(format!("não há conta de \"{host}\"")))?;
    let (status, info, error) = match hosts.account_info(&host, &acc).await {
        Ok(info) if info.premium => (AccountStatus::Premium, info, None),
        Ok(info) => (AccountStatus::Free, info, None),
        Err(HostError::Account(m)) => (AccountStatus::Invalid, AccountInfo::default(), Some(m)),
        Err(e) => (
            AccountStatus::Error,
            AccountInfo::default(),
            Some(e.to_string()),
        ),
    };
    store
        .call(move |c| accounts::set_check(c, &host, status, &info, error.as_deref()))
        .await?;
    let _ = pushes.send(Push::Changed);
    Ok(status)
}

#[cfg(test)]
#[path = "accounts_tests.rs"]
mod tests;
