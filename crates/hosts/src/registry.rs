//! Registro dos plugins: escolhe o plugin pelo link, cai no link direto
//! quando nenhum serve, e implementa o `Resolver` que o motor usa.

use crate::accounts::Accounts;
use crate::direct::{self, DirectResolver};
use crate::dump::Dump;
use crate::plugin::{FileInfo, HostCtx, HostPlugin};
use crate::rules::Rules;
use crate::{detect, gdrive, mediafire, pixeldrain, rapidgator, xfs};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use swoop_core::{
    Account, AccountInfo, AccountKind, ErrorClass, HostError, HostPolicy, HttpFailure,
    ResolveRequest, Resolved, Resolver, default_classify,
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
                accounts: Accounts::default(),
                diag: None,
            },
            plugins: vec![
                Box::new(pixeldrain::Pixeldrain),
                Box::new(mediafire::Mediafire),
                Box::new(gdrive::Gdrive),
                // Antes do XFS: um site XFS nunca reivindica /file/<id> do Rapidgator.
                Box::new(rapidgator::Rapidgator),
                Box::new(xfs::Xfs),
            ],
        })
    }

    /// Guarda em `dir` as páginas que os plugins não entenderem.
    pub fn diagnostics_to(mut self, dir: PathBuf) -> Self {
        self.ctx.diag = Some(Arc::new(crate::diag::Diagnostics::new(dir)));
        self
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
    /// domínio sem `www.` (a mesma que o `Resolved::direct` e o agendador usam).
    pub fn host_key(&self, url: &Url) -> String {
        match self.plugin_for(url) {
            Some(p) => p.host_key(url),
            None => url
                .host_str()
                .unwrap_or(DIRECT)
                .trim_start_matches("www.")
                .to_ascii_lowercase(),
        }
    }

    /// Links num texto qualquer.
    pub fn detect(&self, text: &str) -> Vec<Url> {
        let r = &self.ctx.rules;
        let mut hosts: Vec<&str> = [
            &r.pixeldrain.hosts,
            &r.mediafire.hosts,
            &r.gdrive.hosts,
            &r.xfs.hosts,
        ]
        .into_iter()
        .flatten()
        .map(String::as_str)
        .collect();
        for p in &self.plugins {
            hosts.extend(p.detect_hosts(r));
        }
        detect::detect(text, &hosts)
    }

    /// Contas em uso (o serviço carrega e atualiza; os plugins leem).
    pub fn accounts(&self) -> Accounts {
        self.ctx.accounts.clone()
    }

    /// Servidores que aceitam conta premium.
    pub fn account_hosts(&self) -> Vec<String> {
        let rules = &self.ctx.rules;
        self.plugins
            .iter()
            .flat_map(|p| p.account_hosts(rules))
            .collect()
    }

    /// Tipos de conta aceitos no servidor (vazio = não aceita conta).
    pub fn account_kinds(&self, host_key: &str) -> Vec<AccountKind> {
        self.plugins
            .iter()
            .find(|p| p.accepts_account(host_key, &self.ctx.rules))
            .map(|p| p.account_kinds())
            .unwrap_or_default()
    }

    /// Algum plugin aceita conta para esta chave de servidor?
    pub fn accepts_account(&self, host_key: &str) -> bool {
        self.plugins
            .iter()
            .any(|p| p.accepts_account(host_key, &self.ctx.rules))
    }

    /// Confere a conta no site do servidor.
    pub async fn account_info(
        &self,
        host_key: &str,
        account: &Account,
    ) -> Result<AccountInfo, HostError> {
        let plugin = self
            .plugins
            .iter()
            .find(|p| p.accepts_account(host_key, &self.ctx.rules))
            .ok_or(HostError::Unsupported)?;
        plugin.account_info(&self.ctx, host_key, account).await
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

    fn host_policy(&self, host_key: &str) -> HostPolicy {
        self.plugins
            .iter()
            .find_map(|p| p.host_policy(&self.ctx, host_key))
            .unwrap_or_default()
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
    fn chave_do_link_direto_sem_www() {
        // A mesma chave do agendador e do `Resolved::direct`: as esperas de
        // um servidor valem com ou sem `www.`.
        let r = registry();
        let key = |s: &str| r.host_key(&Url::parse(s).unwrap());
        assert_eq!(key("https://www.Exemplo.com/a.zip"), "exemplo.com");
        assert_eq!(key("https://exemplo.com/b.zip"), "exemplo.com");
        let direct = Resolved::direct(Url::parse("https://www.exemplo.com/a.zip").unwrap());
        assert_eq!(direct.host_key, "exemplo.com");
    }

    #[test]
    fn detecta_com_os_hosts_das_regras() {
        let urls = registry().detect("baixe em pixeldrain.com/u/xyz789 ou drive.google.com/file/d/1AbCdEfGhIjKlMnOpQrStUvWx/view");
        assert_eq!(urls.len(), 2);
    }

    #[test]
    fn rapidgator_em_qualquer_dominio_e_um_servidor_so() {
        let r = registry();
        let id = "0123456789abcdef0123456789abcdef";
        let urls = r.detect(&format!(
            "parte 1: rg.to/file/{id}/a.part1.rar.html parte 2: https://rapidgator.net/file/{id}"
        ));
        assert_eq!(urls.len(), 2);
        for u in &urls {
            assert_eq!(r.host_of(u), "rapidgator");
            assert_eq!(r.host_key(u), "rapidgator");
        }
        let policy = r.host_policy("rapidgator");
        assert_eq!(policy.max_active, Some(1));
        assert!(!policy.ignore_host_waits);
        assert_eq!(r.host_policy("exemplo.com"), HostPolicy::default());
    }
}
