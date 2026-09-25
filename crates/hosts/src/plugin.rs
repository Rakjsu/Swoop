//! Contrato de um plugin de servidor e o que ele recebe para trabalhar.

use crate::dump::Dump;
use crate::rules::Rules;
use async_trait::async_trait;
use std::sync::Arc;
use swoop_core::{ErrorClass, HostError, HttpFailure, ResolveRequest, Resolved, default_classify};
use swoop_net::reqwest;
use url::Url;

/// Níveis de subpasta abertos no máximo ao expandir uma pasta.
pub const MAX_FOLDER_DEPTH: u32 = 5;

/// O que um plugin usa: cliente de páginas (com cookies) e as regras.
#[derive(Clone)]
pub struct HostCtx {
    pub http: reqwest::Client,
    /// Cookies do cliente de páginas (sessão dos sites).
    pub jar: Arc<reqwest::cookie::Jar>,
    pub rules: Arc<Rules>,
    /// Gravação das respostas lidas (`--dump-fixtures`); `None` no uso normal.
    pub dump: Option<Arc<Dump>>,
}

impl HostCtx {
    /// Guarda a resposta lida, se a gravação estiver ligada.
    pub fn record(&self, url: &Url, status: u16, body: &str) {
        if let Some(d) = &self.dump {
            d.save(url, status, body);
        }
    }
}

/// Informação de um arquivo sem baixá-lo (coletor, `swoop-cli check`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    pub name: Option<String>,
    pub size: Option<u64>,
}

/// Um servidor de hospedagem. `matches` é puro; o resto faz rede.
#[async_trait]
pub trait HostPlugin: Send + Sync {
    /// Nome estável: vira o `host_key` (limites e esperas por servidor).
    fn id(&self) -> &'static str;

    /// O link é deste servidor (e de um formato que o plugin entende)?
    fn matches(&self, url: &Url, rules: &Rules) -> bool;

    /// Nome e tamanho, conferindo que o arquivo está online.
    async fn check(&self, ctx: &HostCtx, url: &Url) -> Result<FileInfo, HostError>;

    /// Link direto para o motor (`attempt` > 0 = link anterior expirou).
    /// Link direto para o motor. `req.attempt > 0` = o link anterior
    /// expirou; `req.captcha` traz a resposta do usuário a um captcha pedido.
    async fn resolve(&self, ctx: &HostCtx, req: &ResolveRequest) -> Result<Resolved, HostError>;

    /// Chave do servidor para limites e esperas (o plugin, por padrão; sites
    /// de um mesmo modelo, como o XFS, separam por domínio).
    fn host_key(&self, _url: &Url) -> String {
        self.id().to_owned()
    }

    /// Pasta/lista → links dos arquivos; `None` quando não é pasta.
    async fn expand(&self, _ctx: &HostCtx, _url: &Url) -> Result<Option<Vec<Url>>, HostError> {
        Ok(None)
    }

    /// O que fazer com uma falha HTTP durante o download.
    fn classify(&self, failure: &HttpFailure) -> ErrorClass {
        default_classify(failure)
    }
}
