//! Conta do usuário num servidor. O segredo (senha ou chave de API) mora no
//! cofre do sistema; aqui ele só passa em memória e nunca aparece em log.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Segredo que não se mostra: sem `Display`, e o `Debug` esconde o valor.
/// No JSON é uma string simples.
#[derive(Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(type = "string"))]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// O valor, para mandar ao servidor ou ao cofre (nunca para log).
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_blank(&self) -> bool {
        self.0.trim().is_empty()
    }
}

impl Serialize for Secret {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d).map(Self)
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<oculto>)")
    }
}

/// Como o servidor autentica a conta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    /// Chave da API do site (XFileSharing: "My Account" → API key).
    ApiKey,
    /// Usuário e senha no formulário de login do site.
    Login,
}

impl AccountKind {
    /// Nome estável usado no banco.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ApiKey => "api_key",
            Self::Login => "login",
        }
    }

    /// Lê o nome do banco.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "api_key" => Some(Self::ApiKey),
            "login" => Some(Self::Login),
            _ => None,
        }
    }
}

/// Situação da conta depois da última conferência no site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    /// Ainda não conferida.
    Unchecked,
    Premium,
    /// Aceita pelo site, mas sem premium (ou vencido).
    Free,
    /// O site recusou a chave ou a senha.
    Invalid,
    /// Não deu para conferir (rede, site fora do ar).
    Error,
}

impl AccountStatus {
    /// Nome estável usado no banco.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unchecked => "unchecked",
            Self::Premium => "premium",
            Self::Free => "free",
            Self::Invalid => "invalid",
            Self::Error => "error",
        }
    }

    /// Lê o nome do banco (desconhecido = não conferida).
    pub fn parse(s: &str) -> Self {
        match s {
            "premium" => Self::Premium,
            "free" => Self::Free,
            "invalid" => Self::Invalid,
            "error" => Self::Error,
            _ => Self::Unchecked,
        }
    }
}

/// Conta pronta para um plugin usar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub username: String,
    pub kind: AccountKind,
    pub secret: Secret,
}

/// O que o servidor diz da conta.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountInfo {
    pub premium: bool,
    /// Fim do premium (ms Unix), se o site informa.
    pub premium_until_ms: Option<i64>,
    /// Tráfego restante em bytes, se o site informa.
    pub traffic_left: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segredo_nao_aparece_no_debug_nem_na_conta() {
        let a = Account {
            username: "ana".into(),
            kind: AccountKind::ApiKey,
            secret: Secret::new("chave-super-secreta"),
        };
        assert!(!format!("{a:?}").contains("super-secreta"));
        assert_eq!(a.secret.expose(), "chave-super-secreta");
        // No JSON (vindo da interface) é uma string simples.
        let s: Secret = serde_json::from_str("\"x\"").unwrap();
        assert_eq!(s.expose(), "x");
        assert_eq!(
            AccountKind::parse(AccountKind::Login.as_str()),
            Some(AccountKind::Login)
        );
    }
}
