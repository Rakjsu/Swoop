//! Núcleo de domínio do Swoop: tipos e regras puras, sem rede, disco nem banco.
//!
//! Na fase 0 só expõe a identidade do app; a máquina de estados, `Resolved`,
//! os erros de servidor e as traits entram na fase 1.

/// Nome do produto exibido na interface e usado em pastas de dados.
pub const APP_NAME: &str = "Swoop";

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
