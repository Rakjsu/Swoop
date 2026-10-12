//! Rapidgator (rapidgator.net, rg.to, rapidgator.asia).
//!
//! - grátis (`free`): contador do site respeitado por inteiro, captcha
//!   (Turnstile) resolvido pelo usuário na janela do app, um download por vez
//!   e as esperas do site ("próximo em N min") para o servidor inteiro;
//! - conta premium do usuário (`premium`): API v2, sem contador nem captcha;
//! - pastas (`folder`): lidas da página.
//!
//! Tudo que o site pode mudar fica em `[rapidgator]` nas regras.

mod api;
mod blocked;
mod folder;
mod free;
mod parse;
mod premium;
mod rules;
mod secret;

use crate::page;
use crate::plugin::{FileInfo, HostCtx, HostPlugin};
use crate::rules::Rules;
use async_trait::async_trait;
use std::time::{Duration, SystemTime};
use swoop_core::{
    Account, AccountInfo, AccountKind, HostError, HostPolicy, ResolveRequest, Resolved, WaitReason,
};
use url::Url;

pub use rules::RapidgatorRules;

/// Chave do servidor (esperas e vaga valem para rapidgator.net, rg.to e
/// rapidgator.asia juntos).
const HOST: &str = "rapidgator";

pub struct Rapidgator;

/// Pasta que não abriu quando o link foi adicionado.
const FOLDER_AGAIN: &str = "é uma pasta do Rapidgator que não abriu ao adicionar; cole o link de novo para baixar os arquivos";

/// A conta tem sessão premium aberta e no prazo?
fn premium_now(ctx: &HostCtx, key: &str) -> bool {
    ctx.accounts
        .session(key)
        .is_some_and(|s| s.premium && premium::valid(ctx, &s))
}

/// Chave da conta: o domínio da origem das regras (`rapidgator.net`, com a
/// porta se houver), o mesmo que a aba Contas mostra.
fn account_key(rules: &Rules) -> Option<String> {
    let origin = Url::parse(&rules.rapidgator.origin).ok()?;
    let host = origin.host_str()?.trim_start_matches("www.").to_owned();
    Some(match origin.port() {
        Some(p) => format!("{host}:{p}"),
        None => host,
    })
}

#[async_trait]
impl HostPlugin for Rapidgator {
    fn id(&self) -> &'static str {
        HOST
    }

    fn matches(&self, url: &Url, rules: &Rules) -> bool {
        parse::file_target(url, &rules.rapidgator).is_some()
            || parse::folder_target(url, &rules.rapidgator).is_some()
    }

    fn host_key(&self, _url: &Url) -> String {
        HOST.into()
    }

    async fn check(&self, ctx: &HostCtx, url: &Url) -> Result<FileInfo, HostError> {
        let none = FileInfo {
            name: None,
            size: None,
        };
        let Some((_, file_url)) = parse::file_target(url, &ctx.rules.rapidgator) else {
            // Pasta que não abriu ao adicionar: removida = offline; senão o
            // usuário cola de novo para abrir os arquivos.
            if let Some(dir) = parse::folder_target(url, &ctx.rules.rapidgator) {
                let origin = free::origin(ctx)?;
                let page = page::get_from(ctx, &dir, &origin, false, None).await?;
                folder::folder_page(page.status, &page.body, &page.url, &ctx.rules.rapidgator)?;
                return Err(HostError::BrowserRequired(FOLDER_AGAIN.into()));
            }
            return Ok(none);
        };
        let origin = free::origin(ctx)?;
        let first = page::get_from(ctx, &file_url, &origin, false, None).await?;
        if first.file {
            return Ok(none);
        }
        let rules = &ctx.rules.rapidgator;
        match parse::blocked(first.status, &first.body, rules, SystemTime::now()) {
            // Espera ou limite do grátis não quer dizer que o arquivo sumiu
            // (com conta premium ele baixa; sem, o download diz o motivo).
            Err(HostError::Wait { .. } | HostError::PremiumOnly(_)) | Ok(()) => {}
            Err(e) => return Err(e),
        }
        let (name, size) = parse::file_info(&first.body, rules)?;
        Ok(FileInfo { name, size })
    }

    async fn resolve(&self, ctx: &HostCtx, req: &ResolveRequest) -> Result<Resolved, HostError> {
        let Some((id, file_url)) = parse::file_target(&req.url, &ctx.rules.rapidgator) else {
            return Err(
                match parse::folder_target(&req.url, &ctx.rules.rapidgator) {
                    Some(_) => HostError::BrowserRequired(FOLDER_AGAIN.into()),
                    None => HostError::Unsupported,
                },
            );
        };
        if let Some(key) = account_key(&ctx.rules)
            && let Some(acc) = ctx.accounts.get(&key)
        {
            // O agendador deixou entrar como premium (sem esperas): se a conta
            // deixou de ser premium, volta para a fila em vez de ir pelo grátis.
            let promised = premium_now(ctx, &key);
            if let Some(r) = premium::resolve(ctx, &key, &acc, &id, &file_url).await? {
                return Ok(r);
            }
            if promised {
                return Err(HostError::Wait {
                    until: SystemTime::now() + Duration::from_secs(60),
                    reason: WaitReason::Backoff,
                });
            }
        }
        free::resolve(ctx, req, &file_url).await
    }

    async fn expand(&self, ctx: &HostCtx, url: &Url) -> Result<Option<Vec<Url>>, HostError> {
        match parse::folder_target(url, &ctx.rules.rapidgator) {
            Some(dir) => Ok(Some(folder::expand(ctx, dir).await?)),
            None => Ok(None),
        }
    }

    fn host_policy(&self, ctx: &HostCtx, host_key: &str) -> Option<HostPolicy> {
        if host_key != HOST {
            return None;
        }
        let free = HostPolicy {
            max_active: Some(ctx.rules.rapidgator.free_max_active),
            ignore_host_waits: false,
        };
        let Some(key) = account_key(&ctx.rules).filter(|k| ctx.accounts.get(k).is_some()) else {
            return Some(free);
        };
        Some(if premium_now(ctx, &key) {
            // premium não tem as esperas nem o "um por vez" do grátis
            HostPolicy {
                max_active: None,
                ignore_host_waits: true,
            }
        } else {
            // conta grátis, sessão vencida ou ainda sem login: regras do
            // grátis (o login acontece no primeiro download que entrar)
            free
        })
    }

    fn detect_hosts<'r>(&self, rules: &'r Rules) -> Vec<&'r str> {
        rules.rapidgator.hosts.iter().map(String::as_str).collect()
    }

    fn accepts_account(&self, host_key: &str, rules: &Rules) -> bool {
        account_key(rules).is_some_and(|k| k == host_key)
    }

    fn account_hosts(&self, rules: &Rules) -> Vec<String> {
        account_key(rules).into_iter().collect()
    }

    fn account_kinds(&self) -> Vec<AccountKind> {
        vec![AccountKind::Login]
    }

    async fn account_info(
        &self,
        ctx: &HostCtx,
        host_key: &str,
        account: &Account,
    ) -> Result<AccountInfo, HostError> {
        premium::info(ctx, host_key, account).await
    }
}
