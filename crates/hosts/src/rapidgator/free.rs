//! Rapidgator grátis, rede: página do arquivo → contador (`AjaxStartTimer`)
//! → espera inteira → link pedido → página do captcha → resposta do usuário
//! → link final. O captcha só aparece depois do contador (como no
//! navegador) e a resposta nunca sai depois do prazo do site.

use super::parse::{self, CaptchaPage, Final};
use super::secret::redact;
use super::{HOST, RapidgatorRules};
use crate::flow::{now_ms, session_resolved, wait_until};
use crate::page::{self, Page};
use crate::plugin::HostCtx;
use std::time::SystemTime;
use swoop_core::{
    CaptchaAnswer, CaptchaChallenge, HostError, ResolveRequest, Resolved, WaitReason,
};
use swoop_net::reqwest::cookie::CookieStore;
use url::Url;

/// Origem dos pedidos (com o cookie de idioma no jar).
pub fn origin(ctx: &HostCtx) -> Result<Url, HostError> {
    let rules = &ctx.rules.rapidgator;
    let origin = Url::parse(&rules.origin)
        .map_err(|_| HostError::Changed(format!("origem inválida nas regras: {}", rules.origin)))?;
    ctx.jar
        .add_cookie_str(&format!("{}; Path=/", rules.cookie), &origin);
    Ok(origin)
}

/// Caminho das regras (ou do JavaScript da página) dentro do site.
fn at(origin: &Url, path: &str, query: (&str, &str)) -> Result<Url, HostError> {
    let mut url = origin
        .join(path)
        .map_err(|_| HostError::Changed(format!("caminho inválido do Rapidgator: {path}")))?;
    url.query_pairs_mut().append_pair(query.0, query.1);
    Ok(url)
}

/// Link direto do grátis: uma conexão, sem retomada, com a sessão.
fn resolved(ctx: &HostCtx, link: Url, referer: &Url) -> Resolved {
    let rules = &ctx.rules.rapidgator;
    session_resolved(
        ctx,
        link,
        referer,
        rules.free_connections,
        rules.free_resumable,
        HOST.into(),
    )
}

/// Página não entendida fica salva (sem sessão) para diagnóstico.
fn changed(ctx: &HostCtx, step: &str, page: &Page, e: HostError) -> HostError {
    ctx.changed(HOST, step, &redact(&page.body), e)
}

/// Download grátis do arquivo `file_url` (já normalizado).
pub async fn resolve(
    ctx: &HostCtx,
    req: &ResolveRequest,
    file_url: &Url,
) -> Result<Resolved, HostError> {
    let rules = &ctx.rules.rapidgator;
    if let Some(a) = &req.captcha
        && let Some(r) = answer(ctx, a).await?
    {
        return Ok(r);
    }
    let origin = origin(ctx)?;
    let first = page::get_from(ctx, file_url, &origin, false, None).await?;
    if first.file {
        return Ok(resolved(ctx, first.url, file_url));
    }
    let fp = parse::file_page(first.status, &first.body, rules, SystemTime::now())
        .map_err(|e| changed(ctx, "pagina", &first, e))?;
    if !fp.secs_found {
        tracing::info!(
            secs = fp.secs,
            "rapidgator: contador não achado na página; esperando o padrão das regras"
        );
    }

    let timer = page::get_from(
        ctx,
        &at(&origin, &fp.start_timer, ("fid", &fp.fid))?,
        file_url,
        true,
        Some(redact),
    )
    .await?;
    let sid = parse::timer(timer.status, &timer.body, rules, SystemTime::now())
        .map_err(|e| changed(ctx, "contador", &timer, e))?;
    tracing::info!(secs = fp.secs, "rapidgator: contador iniciado");
    wait_until(now_ms() + ((fp.secs + rules.countdown_margin_secs) * 1000) as i64).await;

    let link = page::get_from(
        ctx,
        &at(&origin, &fp.get_link, ("sid", &sid))?,
        file_url,
        true,
        Some(redact),
    )
    .await?;
    parse::link_ready(link.status, &link.body, rules, SystemTime::now())
        .map_err(|e| changed(ctx, "link", &link, e))?;
    tracing::info!("rapidgator: link pedido");

    let captcha_url = origin
        .join(&fp.captcha)
        .map_err(|_| HostError::Changed("caminho do captcha inválido".into()))?;
    let cp = page::get_from(ctx, &captcha_url, file_url, false, Some(redact)).await?;
    if cp.file {
        return Ok(resolved(ctx, cp.url, &captcha_url));
    }
    match parse::captcha_page(cp.status, &cp.body, &cp.url, rules, SystemTime::now())
        .map_err(|e| changed(ctx, "captcha", &cp, e))?
    {
        CaptchaPage::Link(link) => Ok(resolved(ctx, link, &captcha_url)),
        CaptchaPage::Form { form, kind } => {
            let now = now_ms();
            let fields = form
                .fields
                .iter()
                .filter(|(k, _)| !rules.answer_fields.contains(k))
                .cloned()
                .collect();
            Err(HostError::Captcha(Box::new(CaptchaChallenge {
                kind,
                page_url: file_url.clone(),
                action: form.target(&cp.url),
                fields,
                answer_field: rules.answer_fields.first().cloned().unwrap_or_default(),
                not_before_ms: now,
                expires_at_ms: Some(now + (rules.captcha_deadline_secs * 1000) as i64),
            })))
        }
    }
}

/// Envia a resposta do usuário. `None` = não deu para usar (prazo, sessão
/// perdida, resposta recusada): o fluxo recomeça e pede outro captcha.
async fn answer(ctx: &HostCtx, a: &CaptchaAnswer) -> Result<Option<Resolved>, HostError> {
    let rules = &ctx.rules.rapidgator;
    let origin = origin(ctx)?;
    let left_ms = a.challenge.expires_at_ms.map_or(i64::MAX, |t| t - now_ms());
    if left_ms < (rules.captcha_min_left_secs * 1000) as i64 {
        tracing::info!("rapidgator: prazo do captcha no fim; não envio, recomeço");
        return Ok(None);
    }
    if !has_session(ctx, &origin, rules) {
        tracing::info!("rapidgator: sessão do site perdida; não envio, recomeço");
        return Ok(None);
    }
    let mut fields = a.challenge.fields.clone();
    for name in &rules.answer_fields {
        fields.push((name.clone(), a.token.clone()));
    }
    tracing::info!("rapidgator: enviando o captcha");
    let res = page::post_form_with(ctx, &a.challenge.action, &fields, Some(redact)).await?;
    if late(ctx, &origin, rules) {
        tracing::warn!("rapidgator: o site disse que a resposta chegou tarde");
        return Err(HostError::Wait {
            until: SystemTime::now()
                + std::time::Duration::from_secs(rules.late_captcha_wait_minutes * 60),
            reason: WaitReason::HostLimit,
        });
    }
    if res.file {
        return Ok(Some(resolved(ctx, res.url, &a.challenge.action)));
    }
    match parse::final_page(res.status, &res.body, &res.url, rules, SystemTime::now())
        .map_err(|e| changed(ctx, "final", &res, e))?
    {
        Final::Link(link) => Ok(Some(resolved(ctx, link, &a.challenge.action))),
        Final::Again => {
            tracing::info!("rapidgator: resposta recusada; outro captcha");
            Ok(None)
        }
    }
}

/// Cookies do site no jar, como texto.
fn cookies(ctx: &HostCtx, origin: &Url) -> String {
    ctx.jar
        .cookies(origin)
        .and_then(|v| v.to_str().ok().map(str::to_owned))
        .unwrap_or_default()
}

/// O site ainda reconhece a sessão do contador (algum cookie além do de
/// idioma)? Ao reabrir o app o jar volta vazio.
fn has_session(ctx: &HostCtx, origin: &Url, rules: &RapidgatorRules) -> bool {
    cookies(ctx, origin)
        .split("; ")
        .any(|c| !c.is_empty() && c != rules.cookie)
}

/// O site marcou a resposta como atrasada (`failed_on_captcha=1`)?
fn late(ctx: &HostCtx, origin: &Url, rules: &RapidgatorRules) -> bool {
    cookies(ctx, origin)
        .split("; ")
        .any(|c| c == rules.late_captcha_cookie)
}
