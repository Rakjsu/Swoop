//! Registro dos plugins: escolhe o plugin pelo link, cai no link direto
//! quando nenhum serve, e implementa o `Resolver` que o motor usa.

use crate::direct::{self, DirectResolver};
use crate::dump::Dump;
use crate::plugin::{FileInfo, HostCtx, HostPlugin};
use crate::rules::Rules;
use crate::{detect, gdrive, mediafire, pixeldrain, xfs};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use swoop_core::{
    ErrorClass, HostError, HttpFailure, ResolveRequest, Resolved, Resolver, default_classify,
};
use swoop_net::reqwest;
use url::Url;

/// Nome do "servidor" de links diretos.
pub const DIRECT: &str = "direct";

/// Plugins conhecidos + contexto compartilhado.
pub struct Registry {
    ctx: HostCtx,
    plugins: Vec<Box<dyn HostPlugin>>,
}

impl Registry {
    /// Registro com todos os plugins e as regras dadas.
    pub fn new(rules: Rules) -> Result<Self, reqwest::Error> {
        let jar = Arc::new(reqwest::cookie::Jar::default());
        let http = swoop_net::page_client(&rules.user_agent, jar.clone())?;
        Ok(Self {
            ctx: HostCtx {
                http,
                jar,
                rules: Arc::new(rules),
                dump: None,
            },
            plugins: vec![
                Box::new(pixeldrain::Pixeldrain),
                Box::new(mediafire::Mediafire),
                Box::new(gdrive::Gdrive),
                Box::new(xfs::Xfs),
            ],
        })
    }

    /// Grava em `dir` cada resposta que os plugins lerem (fixtures).
    pub fn dump_to(mut self, dir: PathBuf) -> Self {
        self.ctx.dump = Some(Arc::new(Dump::new(dir)));
        self
    }

    /// Plugin do link, se algum reconhecer.
    pub fn plugin_for(&self, url: &Url) -> Option<&dyn HostPlugin> {
        self.plugins
            .iter()
            .map(AsRef::as_ref)
            .find(|p| p.matches(url, &self.ctx.rules))
    }

    /// Servidor do link (`direct` quando nenhum plugin reconhece).
    pub fn host_of(&self, url: &Url) -> &'static str {
        self.plugin_for(url).map_or(DIRECT, HostPlugin::id)
    }

    /// Chave do servidor para limites: o plugin ou, no link direto, o
    /// domínio (a mesma que o `Resolved::direct` usa).
    pub fn host_key(&self, url: &Url) -> String {
        match self.plugin_for(url) {
            Some(p) => p.host_key(url),
            None => url.host_str().unwrap_or(DIRECT).to_ascii_lowercase(),
        }
    }

    /// Links num texto qualquer.
    pub fn detect(&self, text: &str) -> Vec<Url> {
        let r = &self.ctx.rules;
        let hosts: Vec<&str> = [
            &r.pixeldrain.hosts,
            &r.mediafire.hosts,
            &r.gdrive.hosts,
            &r.xfs.hosts,
        ]
        .into_iter()
        .flatten()
        .map(String::as_str)
        .collect();
        detect::detect(text, &hosts)
    }

    /// Nome e tamanho, conferindo que está online.
    pub async fn check(&self, url: &Url) -> Result<FileInfo, HostError> {
        match self.plugin_for(url) {
            Some(p) => p.check(&self.ctx, url).await,
            None => direct::check(&self.ctx.http, url).await,
        }
    }

    /// Pasta/lista → arquivos; `None` quando o link é de um arquivo só.
    pub async fn expand(&self, url: &Url) -> Result<Option<Vec<Url>>, HostError> {
        match self.plugin_for(url) {
            Some(p) => p.expand(&self.ctx, url).await,
            None => Ok(None),
        }
    }
}

#[async_trait]
impl Resolver for Registry {
    async fn resolve(&self, req: &ResolveRequest) -> Result<Resolved, HostError> {
        match self.plugin_for(&req.url) {
            Some(p) => p.resolve(&self.ctx, req).await,
            None => DirectResolver.resolve(req).await,
        }
    }

    fn classify(&self, host_key: &str, failure: &HttpFailure) -> ErrorClass {
        self.plugins
            .iter()
            .find(|p| p.id() == host_key)
            .map_or_else(|| default_classify(failure), |p| p.classify(failure))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> Registry {
        Registry::new(Rules::embedded()).unwrap()
    }

    #[test]
    fn escolhe_o_plugin_pelo_link() {
        let r = registry();
        let host = |s: &str| r.host_of(&Url::parse(s).unwrap());
        assert_eq!(host("https://pixeldrain.com/u/abc123"), "pixeldrain");
        assert_eq!(
            host("https://www.mediafire.com/file/abc123def/a.zip/file"),
            "mediafire"
        );
        assert_eq!(
            host("https://drive.google.com/file/d/1AbCdEfGhIjKlMnOpQrStUvWx/view"),
            "gdrive"
        );
        assert_eq!(host("https://exemplo.com/a.zip"), DIRECT);
        // página inicial do servidor não é arquivo: fica como link comum
        assert_eq!(host("https://pixeldrain.com/"), DIRECT);
    }

    #[test]
    fn detecta_com_os_hosts_das_regras() {
        let urls = registry().detect("baixe em pixeldrain.com/u/xyz789 ou drive.google.com/file/d/1AbCdEfGhIjKlMnOpQrStUvWx/view");
        assert_eq!(urls.len(), 2);
    }
}
