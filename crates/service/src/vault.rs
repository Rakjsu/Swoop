//! Cofre do sistema para os segredos das contas: Credential Manager no
//! Windows, Keychain no macOS, keyutils no Linux. O banco guarda só o nome da
//! entrada; o segredo nunca passa por log (`Secret` esconde no `Debug`).

use std::sync::Arc;
use swoop_core::Secret;

// Nos testes da unidade o cofre é o de memória; o do sistema fica sem uso.

/// Serviço das entradas no cofre (o mesmo identificador do app).
#[cfg_attr(test, allow(dead_code))]
const SERVICE: &str = swoop_core::APP_IDENTIFIER;

/// Falha do cofre, em português (o detalhe do sistema entre parênteses,
/// nunca o segredo).
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct VaultError(String);

/// Mensagem para o usuário: o que falhou e o motivo do sistema.
#[cfg_attr(test, allow(dead_code))]
fn fail(what: &str, e: keyring::Error) -> VaultError {
    VaultError(format!(
        "não consegui {what} a senha ou chave no cofre do sistema ({e})"
    ))
}

/// Onde os segredos ficam guardados.
pub trait Vault: Send + Sync {
    fn put(&self, name: &str, secret: &Secret) -> Result<(), VaultError>;
    fn get(&self, name: &str) -> Result<Option<Secret>, VaultError>;
    fn delete(&self, name: &str) -> Result<(), VaultError>;
}

/// Cofre do sistema operacional.
#[cfg_attr(test, allow(dead_code))]
pub struct SystemVault;

#[cfg_attr(test, allow(dead_code))]
fn entry(name: &str) -> Result<keyring::Entry, VaultError> {
    keyring::Entry::new(SERVICE, name).map_err(|e| fail("abrir", e))
}

impl Vault for SystemVault {
    fn put(&self, name: &str, secret: &Secret) -> Result<(), VaultError> {
        entry(name)?
            .set_password(secret.expose())
            .map_err(|e| fail("guardar", e))
    }

    fn get(&self, name: &str) -> Result<Option<Secret>, VaultError> {
        match entry(name)?.get_password() {
            Ok(p) => Ok(Some(Secret::new(p))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(fail("ler", e)),
        }
    }

    fn delete(&self, name: &str) -> Result<(), VaultError> {
        match entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(fail("apagar", e)),
        }
    }
}

/// Cofre usado pelo serviço: o do sistema; nos testes da unidade, um em
/// memória, único no processo (reabrir o serviço acha os segredos).
pub fn default_vault() -> Arc<dyn Vault> {
    #[cfg(not(test))]
    {
        Arc::new(SystemVault)
    }
    #[cfg(test)]
    {
        memory::shared()
    }
}

#[cfg(test)]
pub mod memory {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Cofre de teste, em memória.
    #[derive(Default)]
    pub struct MemoryVault(Mutex<HashMap<String, Secret>>);

    /// O cofre de teste do processo (o mesmo que o serviço usa).
    pub fn shared() -> Arc<MemoryVault> {
        use std::sync::OnceLock;
        static MEMORY: OnceLock<Arc<MemoryVault>> = OnceLock::new();
        MEMORY.get_or_init(Default::default).clone()
    }

    impl MemoryVault {
        /// Entradas guardadas (para conferir que a troca/remoção limpa).
        pub fn contains(&self, name: &str) -> bool {
            self.0.lock().unwrap().contains_key(name)
        }
    }

    impl Vault for MemoryVault {
        fn put(&self, name: &str, secret: &Secret) -> Result<(), VaultError> {
            self.0.lock().unwrap().insert(name.into(), secret.clone());
            Ok(())
        }

        fn get(&self, name: &str) -> Result<Option<Secret>, VaultError> {
            Ok(self.0.lock().unwrap().get(name).cloned())
        }

        fn delete(&self, name: &str) -> Result<(), VaultError> {
            self.0.lock().unwrap().remove(name);
            Ok(())
        }
    }
}
