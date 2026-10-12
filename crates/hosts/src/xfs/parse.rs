//! XFileSharing, parte pura: formatos de link, a página do arquivo
//! (formulário `download1`), a página do download grátis (contador, captcha,
//! formulário `download2`) e a página final (link direto ou erro).

use super::scan;
pub use crate::form::Form;
use crate::page::{self, human_size};
use crate::rules::XfsRules;
use regex::Regex;
use scraper::{ElementRef, Html};
use std::time::{Duration, SystemTime};
use swoop_core::{CaptchaKind, HostError, WaitReason};
use url::Url;

/// Código do arquivo no link: `/{code}`, `/{code}/nome`, `/{code}.html`,
/// `/embed-{code}.html`, `/d/{code}`, `/f/{code}`, `/file/{code}`.
pub fn code(url: &Url, rules: &XfsRules) -> Option<String> {
    if !crate::Rules::host_in(&rules.hosts, url.host_str()?) {
        return None;
    }
    let segs: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    let raw = match segs.as_slice() {
        ["d" | "f" | "file", c, ..] => *c,
        [c, ..] => *c,
        [] => return None,
    };
    let c = raw.strip_prefix("embed-").unwrap_or(raw);
    let c = c.strip_suffix(".html").unwrap_or(c).to_ascii_lowercase();
    let re = Regex::new(&rules.code_pattern).ok()?;
    re.is_match(&c).then_some(c)
}

/// O que a página pede agora.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Página do arquivo: enviar o formulário do download grátis.
    Download1 {
        form: Form,
        name: Option<String>,
        size: Option<u64>,
    },
    /// Página do download grátis: contador e, talvez, captcha.
    Download2(Free),
}

/// Página do download grátis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Free {
    pub form: Form,
    pub countdown_secs: u64,
    /// O contador foi lido na página (senão vale a espera padrão das regras).
    pub countdown_found: bool,
    /// Captcha e o campo da resposta.
    pub captcha: Option<(CaptchaKind, String)>,
}

/// Lê uma página do fluxo (a do arquivo ou a do download grátis).
pub fn step(
    html: &str,
    page_url: &Url,
    rules: &XfsRules,
    now: SystemTime,
) -> Result<Step, HostError> {
    blocked(html, rules, now)?;
    let doc = Html::parse_document(html);
    if let Some(form) = form_with_op(&doc, &rules.download2_op)? {
        return Ok(Step::Download2(free(&doc, form, page_url, rules)?));
    }
    let Some(mut form) = form_with_op(&doc, &rules.download1_op)? else {
        return Err(HostError::Changed(
            "página do XFileSharing sem o formulário de download".into(),
        ));
    };
    form.fields.retain(|(k, _)| !k.starts_with("method_"));
    form.fields
        .push((rules.free_field.clone(), rules.free_value.clone()));
    let name = form
        .fields
        .iter()
        .find(|(k, _)| k == "fname")
        .map(|(_, v)| v.clone());
    let size = Regex::new(&rules.size_pattern).ok().and_then(|re| {
        re.captures(html)?
            .get(1)
            .and_then(|m| human_size(m.as_str()))
    });
    Ok(Step::Download1 { form, name, size })
}

/// Desfecho do envio do captcha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Final {
    Link(Url),
    /// Captcha errado, contador pulado ou sessão perdida: pedir de novo.
    Again,
}

/// Página depois do `download2` (quando não veio redirecionamento).
pub fn final_page(
    html: &str,
    page_url: &Url,
    rules: &XfsRules,
    now: SystemTime,
) -> Result<Final, HostError> {
    blocked(html, rules, now)?;
    let lower = html.to_lowercase();
    let has = |m: &[String]| m.iter().any(|x| lower.contains(x.as_str()));
    if has(&rules.wrong_captcha_markers) || has(&rules.skipped_countdown_markers) {
        return Ok(Final::Again);
    }
    let doc = Html::parse_document(html);
    if let Some(href) = page::attr(&doc, &rules.direct_link_selector, "href")?
        && let Ok(link) = page_url.join(href.trim())
        && matches!(link.scheme(), "http" | "https")
    {
        return Ok(Final::Link(link));
    }
    if form_with_op(&doc, &rules.download2_op)?.is_some()
        || form_with_op(&doc, &rules.download1_op)?.is_some()
    {
        return Ok(Final::Again);
    }
    if let Some(link) = scan::final_link(&doc, page_url, rules)? {
        return Ok(Final::Link(link));
    }
    Err(HostError::Changed(
        "página final do XFileSharing sem o link".into(),
    ))
}

/// Situações que param o fluxo em qualquer página.
pub(super) fn blocked(html: &str, rules: &XfsRules, now: SystemTime) -> Result<(), HostError> {
    let lower = html.to_lowercase();
    let has = |m: &[String]| m.iter().any(|x| lower.contains(x.as_str()));
    if has(&rules.cloudflare_markers) {
        return Err(HostError::BrowserRequired(
            "o site pediu a verificação do Cloudflare; abra o link no navegador".into(),
        ));
    }
    if has(&rules.offline_markers) {
        return Err(HostError::Offline);
    }
    if has(&rules.premium_markers) {
        return Err(HostError::PremiumOnly(None));
    }
    if let Some(secs) = wait_secs(&lower, rules) {
        return Err(HostError::Wait {
            until: now + Duration::from_secs(secs + 5),
            reason: WaitReason::HostLimit,
        });
    }
    Ok(())
}

/// "Espere 1 hora, 5 minutos, 12 segundos até o próximo download" → segundos.
fn wait_secs(lower: &str, rules: &XfsRules) -> Option<u64> {
    let caps = Regex::new(&rules.wait_pattern).ok()?.captures(lower)?;
    let n = |i| {
        caps.get(i)
            .and_then(|m| m.as_str().parse::<u64>().ok())
            .unwrap_or(0)
    };
    Some(n(1) * 3600 + n(2) * 60 + n(3))
}

/// Formulário que envia `op = <op>`, com todos os campos (menos botões de
/// outros métodos, que o chamador decide).
pub(super) fn form_with_op(doc: &Html, op: &str) -> Result<Option<Form>, HostError> {
    let forms = page::selector("form")?;
    Ok(doc
        .select(&forms)
        .map(|f| Form::read(f, is_method))
        .find(|f| f.fields.iter().any(|(k, v)| k == "op" && v == op)))
}

/// Botões `method_*` também são campos no XFS (dizem qual download é).
fn is_method(i: &ElementRef<'_>) -> bool {
    i.value()
        .attr("name")
        .is_some_and(|n| n.starts_with("method_"))
}

/// Página do download grátis: contador e captcha.
fn free(doc: &Html, mut form: Form, page_url: &Url, rules: &XfsRules) -> Result<Free, HostError> {
    form.fields.retain(|(k, _)| k != "method_premium");
    let (countdown_secs, countdown_found) = scan::countdown(doc, rules)?;
    if countdown_secs > rules.max_countdown_secs {
        // Encurtar seria enviar antes da hora; esperar às cegas, travar o
        // download. A página fica salva para diagnóstico (`ctx.changed`).
        return Err(HostError::Changed(format!(
            "contador de {countdown_secs} s acima do teto das regras ({} s)",
            rules.max_countdown_secs
        )));
    }
    Ok(Free {
        form,
        countdown_secs,
        countdown_found,
        captcha: captcha(doc, page_url, rules)?,
    })
}

/// Tipo de captcha da página e o campo da resposta.
fn captcha(
    doc: &Html,
    page_url: &Url,
    rules: &XfsRules,
) -> Result<Option<(CaptchaKind, String)>, HostError> {
    if let Some(widget) = crate::flow::widget(doc)? {
        return Ok(Some(widget));
    }
    if let Some(src) = page::attr(doc, &rules.image_captcha_selector, "src")?
        && let Ok(url) = page_url.join(&src)
    {
        return Ok(Some((CaptchaKind::Image { url }, "code".into())));
    }
    let digits = page::selector(&rules.digits_captcha_selector)?;
    if let Some(el) = doc.select(&digits).next() {
        let html = el
            .parent()
            .and_then(ElementRef::wrap)
            .map_or_else(|| el.html(), |p| p.html());
        return Ok(Some((CaptchaKind::Html { html }, "code".into())));
    }
    Ok(None)
}

#[cfg(test)]
#[path = "parse_tests.rs"]
mod tests;
