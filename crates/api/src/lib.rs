//! Contrato compartilhado entre o motor e quem o controla: a janela Tauri e,
//! a partir da v0.3.0, o painel web (e depois a extensão de navegador).
//!
//! - `command`: pedidos da interface (`Command`) e respostas (`Reply`);
//! - `view`: o que a interface mostra (linhas da fila, retrato ao vivo, avisos);
//! - `backend`: a trait `Backend`, única porta de entrada das interfaces.
//!
//! Os tipos TypeScript saem daqui pelo `ts-rs` (`cargo test -p swoop-api`)
//! para `ui/src/gen`; a UI nunca redeclara esses tipos à mão.

mod accounts;
mod backend;
mod command;
mod view;

pub use accounts::{AccountView, AccountsView};
pub use backend::{ApiError, Backend, Subscription};
pub use command::{Command, Reply};
pub use swoop_core::Settings;
pub use view::{
    CaptchaInfo, CollectorView, DownloadView, HistoryOutcome, HistoryView, LiveRow, Notice, Push,
    Snapshot,
};

use serde::Serialize;
use ts_rs::TS;

/// Identidade do app devolvida para a interface (tela "Sobre", título).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
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
