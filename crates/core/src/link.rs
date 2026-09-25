//! Estado de um link no coletor (antes de virar download).

use serde::{Deserialize, Serialize};

/// Onde está a verificação de um link colado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum LinkState {
    /// Ainda não verificado.
    Unchecked,
    /// Verificação em andamento.
    Checking,
    /// O servidor confirmou nome/tamanho.
    Online,
    /// Removido ou inexistente.
    Offline,
    /// Não deu para verificar (rede, captcha, servidor mudou); pode iniciar
    /// mesmo assim.
    Failed,
}

impl LinkState {
    const ALL: [Self; 5] = [
        Self::Unchecked,
        Self::Checking,
        Self::Online,
        Self::Offline,
        Self::Failed,
    ];

    /// Nome estável usado no banco.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unchecked => "unchecked",
            Self::Checking => "checking",
            Self::Online => "online",
            Self::Offline => "offline",
            Self::Failed => "failed",
        }
    }

    /// Lê o nome gravado no banco.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|st| st.as_str() == s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nome_no_banco_ida_e_volta() {
        for st in LinkState::ALL {
            assert_eq!(LinkState::parse(st.as_str()), Some(st));
        }
        assert_eq!(LinkState::parse("x"), None);
    }
}
