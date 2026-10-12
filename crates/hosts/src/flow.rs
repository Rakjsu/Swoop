//! Peças comuns dos fluxos com sessão no site (XFileSharing, Rapidgator):
//! relógio, espera inteira do contador, widget de captcha da página e o link
//! final com a sessão (cookies, Referer e o mesmo navegador).

use crate::page;
use crate::plugin::HostCtx;
use scraper::Html;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use swoop_core::{CaptchaKind, HostError, RangeStyle, Resolved};
use swoop_net::reqwest::cookie::CookieStore;
use url::Url;

/// Agora em ms Unix.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Espera até `until_ms`, inteira (o contador do site nunca é encurtado).
pub async fn wait_until(until_ms: i64) {
    let left = (until_ms - now_ms()).max(0) as u64;
    if left > 0 {
        tokio::time::sleep(Duration::from_millis(left)).await;
    }
}

/// Link direto com a sessão do site: os cookies do jar, o Referer e o
/// User-Agent das regras (o cliente de download tem outro, e o site amarra o
/// link à sessão). Sem retomada, uma conexão do começo ao fim.
pub fn session_resolved(
    ctx: &HostCtx,
    link: Url,
    referer: &Url,
    connections: u16,
    resumable: bool,
    host_key: String,
) -> Resolved {
    let mut r = Resolved::direct(link.clone());
    r.headers.push(("Referer".into(), referer.to_string()));
    r.headers
        .push(("User-Agent".into(), ctx.rules.user_agent.clone()));
    if let Some(cookie) = ctx
        .jar
        .cookies(&link)
        .and_then(|v| v.to_str().ok().map(str::to_owned))
    {
        r.headers.push(("Cookie".into(), cookie));
    }
    r.max_connections = connections.max(1);
    r.resumable = resumable;
    if !resumable {
        r.range = RangeStyle::None;
    }
    r.host_key = host_key;
    r
}

/// Widget oficial de captcha da página (`data-sitekey`) e o campo da
/// resposta: reCAPTCHA v2, hCaptcha ou Turnstile.
pub fn widget(doc: &Html) -> Result<Option<(CaptchaKind, String)>, HostError> {
    let key = |css: &str| page::attr(doc, css, "data-sitekey");
    if let Some(site_key) = key(".g-recaptcha[data-sitekey]")? {
        return Ok(Some((
            CaptchaKind::Recaptcha2 { site_key },
            "g-recaptcha-response".into(),
        )));
    }
    if let Some(site_key) = key(".h-captcha[data-sitekey]")? {
        return Ok(Some((
            CaptchaKind::Hcaptcha { site_key },
            "h-captcha-response".into(),
        )));
    }
    if let Some(site_key) = key(".cf-turnstile[data-sitekey]")? {
        return Ok(Some((
            CaptchaKind::Turnstile { site_key },
            "cf-turnstile-response".into(),
        )));
    }
    Ok(None)
}
