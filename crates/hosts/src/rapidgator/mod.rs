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
use crate::plugin::{FileInfo, HostCtx, HostPlugin, MAX_FOLDER_DEPTH};
use crate::rules::Rules;
use async_trait::async_trait;
use std::collections::HashSet;
use std::time::{Duration, SystemTime};
use swoop_core::{
    Account, AccountInfo, AccountKind, HostError, HostPolicy, ResolveRequest, Resolved,
};
use url::Url;

pub use rules::RapidgatorRules;

/// Chave do servidor (esperas e vaga valem para rapidgator.net, rg.to e
/// rapidgator.asia juntos).
const HOST: &str = "rapidgator";

pub struct Rapidgator;

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

/// Arquivos da pasta e das subpastas (até `MAX_FOLDER_DEPTH` níveis),
/// página a página, com uma pausa entre as páginas.
async fn expand_folder(ctx: &HostCtx, root: Url) -> Result<Vec<Url>, HostError> {
    let rules = &ctx.rules.rapidgator;
    let origin = free::origin(ctx)?;
    let (mut files, mut seen) = (Vec::new(), HashSet::new());
    let mut todo = vec![(root.clone(), 0)];
    while let Some((dir, depth)) = todo.pop() {
        if !seen.insert(dir.path().to_owned()) {
            continue;
        }
        for n in 1..=rules.folder_max_pages {
            let mut url = dir.clone();
            if n > 1 {
                url.query_pairs_mut().append_pair("page", &n.to_string());
                tokio::time::sleep(Duration::from_millis(rules.folder_page_delay_ms)).await;
            }
            let page = page::get_from(ctx, &url, &origin, false, None).await?;
            let listed = match folder::folder_page(page.status, &page.body, &page.url, rules) {
                Ok(listed) => listed,
                // subpasta que sumiu não derruba a pasta inteira
                Err(HostError::Offline) if dir != root => break,
                Err(e) => return Err(ctx.changed(HOST, "pasta", &page.body, e)),
            };
            for f in listed.files {
                if !files.contains(&f) {
                    files.push(f);
                }
            }
            if depth < MAX_FOLDER_DEPTH {
                todo.extend(listed.folders.into_iter().map(|u| (u, depth + 1)));
            }
            if n >= listed.last_page {
                break;
            }
        }
    }
    Ok(files)
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
        let (id, file_url) =
            parse::file_target(&req.url, &ctx.rules.rapidgator).ok_or(HostError::Unsupported)?;
        if let Some(key) = account_key(&ctx.rules)
            && let Some(acc) = ctx.accounts.get(&key)
            && let Some(r) = premium::resolve(ctx, &key, &acc, &id, &file_url).await?
        {
            return Ok(r);
        }
        free::resolve(ctx, req, &file_url).await
    }

    async fn expand(&self, ctx: &HostCtx, url: &Url) -> Result<Option<Vec<Url>>, HostError> {
        match parse::folder_target(url, &ctx.rules.rapidgator) {
            Some(folder) => Ok(Some(expand_folder(ctx, folder).await?)),
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
        Some(match ctx.accounts.session(&key) {
            // premium não tem as esperas nem o "um por vez" do grátis
            Some(s) if s.premium => HostPolicy {
                max_active: None,
                ignore_host_waits: true,
            },
            Some(_) => free,
            // conta ainda sem login: um download entra para descobrir (se ela
            // não for premium, o site mostra a espera de novo)
            None => HostPolicy {
                max_active: Some(1),
                ignore_host_waits: true,
            },
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
