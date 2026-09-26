//! XFileSharing Pro (fastfile.cc e parecidos), download grátis:
//! página do arquivo → `download1` → contador + captcha → `download2` → link.
//!
//! O contador do site nunca é pulado: o formulário final só sai depois dele.
//! O captcha nunca é resolvido aqui: vira `HostError::Captcha` e o usuário
//! resolve na janela do app; a resposta volta em `ResolveRequest::captcha`.

mod parse;

use crate::page::{self, Page};
use crate::plugin::{FileInfo, HostCtx, HostPlugin};
use crate::rules::{Rules, XfsRules};
use async_trait::async_trait;
use parse::{Final, Step};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use swoop_core::{
    CaptchaAnswer, CaptchaChallenge, HostError, RangeStyle, ResolveRequest, Resolved,
};
use swoop_net::reqwest::cookie::CookieStore;
use url::Url;

pub struct Xfs;

/// Agora em ms Unix.
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Espera até `until_ms` (no máximo o teto de contador das regras).
async fn wait_until(until_ms: i64, rules: &XfsRules) {
    let left = (until_ms - now_ms()).max(0) as u64;
    let left = left.min(rules.max_countdown_secs * 1000);
    if left > 0 {
        tokio::time::sleep(Duration::from_millis(left)).await;
    }
}

impl Xfs {
    /// Link direto pronto para o motor, com os cookies da sessão do site.
    fn resolved(ctx: &HostCtx, link: Url, referer: &Url) -> Resolved {
        let rules = &ctx.rules.xfs;
        let mut r = Resolved::direct(link.clone());
        r.headers.push(("Referer".into(), referer.to_string()));
        if let Some(cookie) = ctx
            .jar
            .cookies(&link)
            .and_then(|v| v.to_str().ok().map(str::to_owned))
        {
            r.headers.push(("Cookie".into(), cookie));
        }
        r.max_connections = rules.free_connections.max(1);
        r.resumable = rules.free_resumable;
        if !rules.free_resumable {
            r.range = RangeStyle::None;
        }
        r.host_key = site_key(referer);
        r
    }

    /// Página do download grátis a partir da página do arquivo.
    async fn free_page(ctx: &HostCtx, url: &Url) -> Result<(Page, Option<parse::Free>), HostError> {
        let rules = &ctx.rules.xfs;
        let first = page::get(ctx, url).await?;
        if first.file {
            return Ok((first, None));
        }
        let free = match parse::step(&first.body, &first.url, rules, SystemTime::now())? {
            Step::Download2(free) => free,
            Step::Download1 { form, .. } => {
                let second = page::post_form(ctx, &form.target(&first.url), &form.fields).await?;
                if second.file {
                    return Ok((second, None));
                }
                match parse::step(&second.body, &second.url, rules, SystemTime::now())? {
                    Step::Download2(free) => free,
                    Step::Download1 { .. } => {
                        return Err(HostError::Changed(
                            "o XFileSharing não abriu o download grátis".into(),
                        ));
                    }
                }
            }
        };
        Ok((first, Some(free)))
    }

    /// Envia o formulário final (com a resposta, se houver) depois do contador.
    async fn submit(
        ctx: &HostCtx,
        action: &Url,
        fields: &[(String, String)],
        referer: &Url,
    ) -> Result<Option<Resolved>, HostError> {
        let res = page::post_form(ctx, action, fields).await?;
        if res.file {
            return Ok(Some(Self::resolved(ctx, res.url, referer)));
        }
        match parse::final_page(&res.body, &res.url, &ctx.rules.xfs, SystemTime::now())? {
            Final::Link(link) => Ok(Some(Self::resolved(ctx, link, referer))),
            Final::Again => Ok(None),
        }
    }

    /// Resposta do usuário: espera o que falta do contador e envia.
    async fn answer(ctx: &HostCtx, a: &CaptchaAnswer) -> Result<Option<Resolved>, HostError> {
        wait_until(a.challenge.not_before_ms, &ctx.rules.xfs).await;
        let mut fields = a.challenge.fields.clone();
        fields.push((a.challenge.answer_field.clone(), a.token.clone()));
        Self::submit(ctx, &a.challenge.action, &fields, &a.challenge.page_url).await
    }
}

/// Chave do servidor: o domínio sem `www.` (esperas valem por site).
fn site_key(url: &Url) -> String {
    url.host_str()
        .unwrap_or("xfs")
        .trim_start_matches("www.")
        .to_ascii_lowercase()
}

#[async_trait]
impl HostPlugin for Xfs {
    fn id(&self) -> &'static str {
        "xfs"
    }

    fn matches(&self, url: &Url, rules: &Rules) -> bool {
        parse::code(url, &rules.xfs).is_some()
    }

    fn host_key(&self, url: &Url) -> String {
        site_key(url)
    }

    async fn check(&self, ctx: &HostCtx, url: &Url) -> Result<FileInfo, HostError> {
        let first = page::get(ctx, url).await?;
        if first.file {
            return Ok(FileInfo {
                name: None,
                size: None,
            });
        }
        match parse::step(&first.body, &first.url, &ctx.rules.xfs, SystemTime::now()) {
            Ok(Step::Download1 { name, size, .. }) => Ok(FileInfo { name, size }),
            Ok(Step::Download2(_)) => Ok(FileInfo {
                name: None,
                size: None,
            }),
            // Espera entre downloads não quer dizer que o arquivo sumiu.
            Err(HostError::Wait { .. }) => Ok(FileInfo {
                name: None,
                size: None,
            }),
            Err(e) => Err(e),
        }
    }

    async fn resolve(&self, ctx: &HostCtx, req: &ResolveRequest) -> Result<Resolved, HostError> {
        if let Some(answer) = &req.captcha
            && let Some(resolved) = Self::answer(ctx, answer).await?
        {
            return Ok(resolved);
        }
        let (page, free) = Self::free_page(ctx, &req.url).await?;
        let Some(free) = free else {
            return Ok(Self::resolved(ctx, page.url, &req.url));
        };
        let not_before_ms = now_ms() + free.countdown_secs as i64 * 1000;
        let action = free.form.target(&page.url);
        match free.captcha {
            Some((kind, answer_field)) => Err(HostError::Captcha(Box::new(CaptchaChallenge {
                kind,
                page_url: req.url.clone(),
                action,
                fields: free.form.fields,
                answer_field,
                not_before_ms,
            }))),
            None => {
                wait_until(not_before_ms, &ctx.rules.xfs).await;
                Self::submit(ctx, &action, &free.form.fields, &req.url)
                    .await?
                    .ok_or_else(|| {
                        HostError::Changed("o XFileSharing recusou o formulário final".into())
                    })
            }
        }
    }
}
