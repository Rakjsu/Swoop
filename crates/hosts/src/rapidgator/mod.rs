//! Rapidgator (rapidgator.net, rg.to, rapidgator.asia).
//!
//! - grátis (`free`): contador do site respeitado por inteiro, captcha
//!   (Turnstile) resolvido pelo usuário na janela do app, um download por vez
//!   e as esperas do site ("próximo em N min") para o servidor inteiro;
//! - conta premium do usuário (`premium`): API v2, sem contador nem captcha;
//! - pastas (`folder`): lidas da página.
//!
//! Tudo que o site pode mudar fica em `[rapidgator]` nas regras.

mod blocked;
mod free;
mod parse;
mod rules;
mod secret;

use crate::page;
use crate::plugin::{FileInfo, HostCtx, HostPlugin};
use crate::rules::Rules;
use async_trait::async_trait;
use std::time::SystemTime;
use swoop_core::{HostError, HostPolicy, ResolveRequest, Resolved};
use url::Url;

pub use rules::RapidgatorRules;

/// Chave do servidor (esperas e vaga valem para rapidgator.net, rg.to e
/// rapidgator.asia juntos).
const HOST: &str = "rapidgator";

pub struct Rapidgator;

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
            return Ok(none);
        };
        let origin = free::origin(ctx)?;
        let first = page::get_from(ctx, &file_url, &origin, false, None).await?;
        if first.file {
            return Ok(none);
        }
        let rules = &ctx.rules.rapidgator;
        match parse::blocked(first.status, &first.body, rules, SystemTime::now()) {
            // Espera do grátis não quer dizer que o arquivo sumiu.
            Err(HostError::Wait { .. }) | Ok(()) => {}
            Err(e) => return Err(e),
        }
        let (name, size) = parse::file_info(&first.body, rules)?;
        Ok(FileInfo { name, size })
    }

    async fn resolve(&self, ctx: &HostCtx, req: &ResolveRequest) -> Result<Resolved, HostError> {
        let (_, file_url) =
            parse::file_target(&req.url, &ctx.rules.rapidgator).ok_or(HostError::Unsupported)?;
        free::resolve(ctx, req, &file_url).await
    }

    fn host_policy(&self, ctx: &HostCtx, host_key: &str) -> Option<HostPolicy> {
        (host_key == HOST).then(|| HostPolicy {
            max_active: Some(ctx.rules.rapidgator.free_max_active),
            ignore_host_waits: false,
        })
    }

    fn detect_hosts<'r>(&self, rules: &'r Rules) -> Vec<&'r str> {
        rules.rapidgator.hosts.iter().map(String::as_str).collect()
    }
}
