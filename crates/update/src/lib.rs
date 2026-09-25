//! Atualização do Swoop pelas Releases do GitHub (sem chave própria de
//! assinatura, decisão do dono em 25/09).
//!
//! Garantias:
//! - só baixa de `https://github.com/Rakjsu/Swoop/releases/download/…`
//!   (HTTPS, com certificado verificado);
//! - confere o sha256 do instalador contra o `SHA256SUMS.txt` da mesma release;
//! - nunca volta para uma versão menor ou igual à instalada.
//!
//! O que NÃO garante: se a conta do GitHub for invadida, uma release falsa
//! seria aceita. A assinatura do updater do Tauri fecharia essa porta, ao
//! custo de guardar uma chave privada nos Secrets do repositório.

pub mod download;
mod release;
mod version;

pub use download::download;
pub use release::{Asset, GhRelease, Update, parse_sums, select};
pub use version::Version;

/// De onde vêm as releases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// Endpoint da API com a última release estável.
    pub api_latest: String,
    /// Prefixo obrigatório das URLs de download dos arquivos.
    pub download_prefix: String,
}

impl Source {
    /// Releases de um repositório do GitHub.
    pub fn github(owner: &str, repo: &str) -> Self {
        Self {
            api_latest: format!("https://api.github.com/repos/{owner}/{repo}/releases/latest"),
            download_prefix: format!("https://github.com/{owner}/{repo}/releases/download/"),
        }
    }

    /// Repositório oficial do Swoop.
    pub fn swoop() -> Self {
        Self::github("Rakjsu", "Swoop")
    }
}

/// Falha ao procurar ou baixar uma atualização (mensagens para o usuário).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UpdateError {
    #[error("falha de rede: {0}")]
    Network(String),
    #[error("o GitHub respondeu HTTP {0}")]
    Http(u16),
    #[error("resposta inesperada do GitHub: {0}")]
    BadResponse(String),
    #[error("a release não tem {0}")]
    Missing(String),
    #[error("endereço de download fora do repositório oficial: {0}")]
    Untrusted(String),
    #[error("o instalador baixado não confere com o sha256 publicado")]
    HashMismatch,
    #[error("falha de disco: {0}")]
    Io(String),
}

/// Consulta a última release e diz se há versão maior que `current`.
pub async fn check(
    client: &swoop_net::reqwest::Client,
    source: &Source,
    current: Version,
) -> Result<Option<Update>, UpdateError> {
    let resp = client
        .get(&source.api_latest)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    match resp.status().as_u16() {
        200 => {}
        // Repositório ainda sem nenhuma release publicada.
        404 => return Ok(None),
        s => return Err(UpdateError::Http(s)),
    }
    let body = resp
        .bytes()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    let release: GhRelease =
        serde_json::from_slice(&body).map_err(|e| UpdateError::BadResponse(e.to_string()))?;
    select(&release, current, source)
}
