//! Núcleo de domínio do Swoop: tipos e regras puras, sem rede, disco nem banco.
//!
//! - `state`: máquina de estados de um download (função pura `next`).
//! - `account`: conta do usuário num servidor (segredo que não aparece em log).
//! - `captcha`: desafio de captcha guardado até o usuário resolver.
//! - `resolved`/`resolver`: o contrato entre plugins de servidor e o motor.
//! - `error`: erros de servidor e a classificação de falhas HTTP.
//! - `filename`: nomes de arquivo seguros vindos de servidores.
//! - `kind`: vídeo, áudio ou outro, para a pasta automática.
//! - `link`: estado de um link no coletor.
//! - `settings`/`units`: preferências e conversão de tamanhos.

pub mod account;
pub mod captcha;
pub mod error;
pub mod filename;
pub mod ids;
pub mod kind;
pub mod link;
pub mod resolved;
pub mod resolver;
pub mod settings;
pub mod state;
pub mod units;

pub use account::{Account, AccountInfo, AccountKind, AccountStatus, Secret};
pub use captcha::{CaptchaAnswer, CaptchaChallenge, CaptchaKind};
pub use error::{ErrorClass, HostError, HttpFailure, WaitReason, default_classify};
pub use ids::{DownloadId, PackageId};
pub use kind::{FileKind, file_kind};
pub use link::LinkState;
pub use resolved::{Integrity, RangeStyle, ResolveRequest, Resolved};
pub use resolver::Resolver;
pub use settings::Settings;
pub use state::{DownloadState, Event, InvalidTransition};

/// Nome do produto exibido na interface e usado em pastas de dados.
pub const APP_NAME: &str = "Swoop";

/// Identificador do app (o mesmo do Tauri): nome da pasta de dados do SO.
pub const APP_IDENTIFIER: &str = "io.github.rakjsu.swoop";

/// Versão do workspace (a mesma em todos os crates e apps).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Texto curto de identificação (`Swoop 0.1.0`) usado por `--version` e logs.
pub fn version_line() -> String {
    format!("{APP_NAME} {VERSION}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_line_tem_nome_e_versao() {
        assert_eq!(
            version_line(),
            format!("Swoop {}", env!("CARGO_PKG_VERSION"))
        );
    }
}
