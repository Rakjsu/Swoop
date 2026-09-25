//! Contrato compartilhado entre o motor e quem o controla: a janela Tauri, o
//! painel web do celular e a extensão de navegador.
//!
//! Na fase 0 só existe `AppInfo`; `Command`, `Snapshot`, `Event` e a trait
//! `Backend` entram na fase 2, com os tipos TypeScript gerados por `ts-rs`.

use serde::Serialize;

/// Identidade do app devolvida para a interface (tela "Sobre", título).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}

impl AppInfo {
    /// Monta a identidade a partir das constantes do núcleo.
    pub fn current() -> Self {
        Self {
            name: swoop_core::APP_NAME.to_owned(),
            version: swoop_core::VERSION.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_serializa_nome_e_versao() {
        let json = serde_json::to_value(AppInfo::current()).unwrap();
        assert_eq!(json["name"], "Swoop");
        assert_eq!(json["version"], swoop_core::VERSION);
    }
}
