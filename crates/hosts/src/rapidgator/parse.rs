//! Rapidgator, parte pura: formato do link, página do arquivo, respostas do
//! contador, página do captcha e página final. Situações do site (espera
//! entre downloads, limite, só premium, offline) viram `HostError` claros.

use super::RapidgatorRules;
pub use super::blocked::blocked;
use crate::flow::widget;
use crate::form::Form;
use crate::page::human_size;
use regex::Regex;
use scraper::Html;
use std::time::{Duration, SystemTime};
use swoop_core::{CaptchaKind, HostError, WaitReason};
use url::Url;

/// Erro de regra inválida (regex quebrada no override do usuário).
pub(super) fn regex(pattern: &str) -> Result<Regex, HostError> {
    Regex::new(pattern)
        .map_err(|_| HostError::Changed(format!("regex inválida nas regras: {pattern}")))
}

/// Link do servidor no caminho de `pattern`, normalizado na origem das
/// regras (rg.to e rapidgator.asia viram rapidgator.net: esquema, host e
/// porta); caminho e query ficam.
fn normalize(url: &Url, rules: &RapidgatorRules, pattern: &str) -> Option<(String, Url)> {
    if !crate::Rules::host_in(&rules.hosts, url.host_str()?) {
        return None;
    }
    let path = url.path().to_ascii_lowercase();
    let id = Regex::new(pattern)
        .ok()?
        .captures(&path)?
        .get(1)?
        .as_str()
        .to_owned();
    let mut canonical = Url::parse(&rules.origin).ok()?.join(url.path()).ok()?;
    canonical.set_query(url.query());
    Some((id, canonical))
}

/// Id do arquivo e o link normalizado (`/file/<id>[/<nome>.html]`).
pub fn file_target(url: &Url, rules: &RapidgatorRules) -> Option<(String, Url)> {
    normalize(url, rules, &rules.file_pattern)
}

/// Link normalizado de uma pasta (`/folder/<n>[/<nome>.html]`).
pub fn folder_target(url: &Url, rules: &RapidgatorRules) -> Option<Url> {
    normalize(url, rules, &rules.folder_pattern).map(|(_, u)| u)
}

/// Página do arquivo: o que o download grátis precisa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePage {
    pub fid: String,
    pub secs: u64,
    /// O contador estava na página (senão vale o padrão das regras).
    pub secs_found: bool,
    pub start_timer: String,
    pub get_link: String,
    pub captcha: String,
    pub name: Option<String>,
    pub size: Option<u64>,
}

/// Nome e tamanho escritos na página (para o coletor).
pub fn file_info(
    html: &str,
    rules: &RapidgatorRules,
) -> Result<(Option<String>, Option<u64>), HostError> {
    let name = regex(&rules.name_pattern)?
        .captures(html)
        .and_then(|c| Some(html_unescape(c.get(1)?.as_str())));
    let size = regex(&rules.size_pattern)?
        .captures(html)
        .and_then(|c| human_size(c.get(1)?.as_str()));
    Ok((name, size))
}

/// Entidades comuns de um `<title>` (o texto não passa pelo parser HTML).
fn html_unescape(s: &str) -> String {
    Html::parse_fragment(s)
        .root_element()
        .text()
        .collect::<String>()
        .trim()
        .to_owned()
}

/// Lê a página do arquivo (variáveis do JavaScript do download grátis).
pub fn file_page(
    status: u16,
    html: &str,
    rules: &RapidgatorRules,
    now: SystemTime,
) -> Result<FilePage, HostError> {
    blocked(status, html, rules, now)?;
    let vars: Vec<(String, String)> = regex(&rules.var_pattern)?
        .captures_iter(html)
        .filter_map(|c| {
            Some((
                c.get(1)?.as_str().to_owned(),
                c.get(2)?.as_str().trim().to_owned(),
            ))
        })
        .collect();
    let var = |k: &str| {
        vars.iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .filter(|v| !v.is_empty())
    };
    let fid = var("fid")
        .filter(|f| f.chars().all(|c| c.is_ascii_alphanumeric()))
        .ok_or_else(|| {
            HostError::Changed("página do Rapidgator sem o download grátis (fid)".into())
        })?;
    let found = var("secs").and_then(|s| s.parse::<u64>().ok());
    let secs = found.unwrap_or(rules.fallback_countdown_secs);
    if secs > rules.max_countdown_secs {
        return Err(HostError::Changed(format!(
            "contador de {secs} s acima do teto das regras ({} s)",
            rules.max_countdown_secs
        )));
    }
    let (name, size) = file_info(html, rules)?;
    Ok(FilePage {
        fid,
        secs,
        secs_found: found.is_some(),
        start_timer: var("startTimerUrl").unwrap_or_else(|| rules.start_timer_path.clone()),
        get_link: var("getDownloadUrl").unwrap_or_else(|| rules.get_link_path.clone()),
        captcha: var("captchaUrl").unwrap_or_else(|| rules.captcha_path.clone()),
        name,
        size,
    })
}

/// Resposta do contador (`{"state":"started","sid":"…"}`) → `sid`.
pub fn timer(
    status: u16,
    body: &str,
    rules: &RapidgatorRules,
    now: SystemTime,
) -> Result<String, HostError> {
    let json = state_json(status, body, rules, now)?;
    let sid = json.get("sid").and_then(|v| v.as_str()).unwrap_or("");
    match json.get("state").and_then(|v| v.as_str()) {
        Some(s) if s == rules.timer_ok && !sid.is_empty() => Ok(sid.to_owned()),
        _ => Err(state_error(rules, now)),
    }
}

/// Resposta do pedido do link (`{"state":"done"}`).
pub fn link_ready(
    status: u16,
    body: &str,
    rules: &RapidgatorRules,
    now: SystemTime,
) -> Result<(), HostError> {
    let json = state_json(status, body, rules, now)?;
    match json.get("state").and_then(|v| v.as_str()) {
        Some(s) if s == rules.link_ok => Ok(()),
        _ => Err(state_error(rules, now)),
    }
}

/// JSON do contador; uma página no lugar dele passa por `blocked`.
fn state_json(
    status: u16,
    body: &str,
    rules: &RapidgatorRules,
    now: SystemTime,
) -> Result<serde_json::Value, HostError> {
    match serde_json::from_str::<serde_json::Value>(body.trim()) {
        Ok(v) if v.is_object() => Ok(v),
        _ => {
            blocked(status, body, rules, now)?;
            Err(HostError::Changed(
                "resposta do contador do Rapidgator não é JSON".into(),
            ))
        }
    }
}

/// Estado inesperado no contador ("You didn`t wait specified time"): espera
/// e recomeça do zero; nunca repete o pedido em laço.
fn state_error(rules: &RapidgatorRules, now: SystemTime) -> HostError {
    HostError::Wait {
        until: now + Duration::from_secs(rules.state_error_wait_minutes * 60),
        reason: WaitReason::Backoff,
    }
}

/// Página do captcha: o link já pronto ou o formulário com o widget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptchaPage {
    Link(Url),
    Form { form: Form, kind: CaptchaKind },
}

/// Lê a página `/download/captcha`.
pub fn captcha_page(
    status: u16,
    html: &str,
    page_url: &Url,
    rules: &RapidgatorRules,
    now: SystemTime,
) -> Result<CaptchaPage, HostError> {
    blocked(status, html, rules, now)?;
    if let Some(link) = final_link(html, page_url, rules)? {
        return Ok(CaptchaPage::Link(link));
    }
    let doc = Html::parse_document(html);
    let form = Form::by_selector(&doc, &rules.captcha_form_selector)?.ok_or_else(|| {
        HostError::Changed("página do captcha do Rapidgator sem o formulário".into())
    })?;
    let kind = match widget(&doc)? {
        Some((kind, _)) => kind,
        None => {
            let key = regex(&rules.turnstile_sitekey_pattern)?
                .captures(html)
                .and_then(|c| Some(c.get(1)?.as_str().to_owned()))
                .ok_or_else(|| {
                    HostError::Changed("captcha do Rapidgator sem a chave do widget".into())
                })?;
            CaptchaKind::Turnstile { site_key: key }
        }
    };
    Ok(CaptchaPage::Form { form, kind })
}

/// Desfecho do envio do captcha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Final {
    Link(Url),
    /// Resposta recusada: começar de novo (outro contador e captcha).
    Again,
}

/// Página depois do envio do captcha.
pub fn final_page(
    status: u16,
    html: &str,
    page_url: &Url,
    rules: &RapidgatorRules,
    now: SystemTime,
) -> Result<Final, HostError> {
    blocked(status, html, rules, now)?;
    if let Some(link) = final_link(html, page_url, rules)? {
        return Ok(Final::Link(link));
    }
    let lower = html.to_lowercase();
    if rules
        .wrong_captcha_markers
        .iter()
        .any(|m| lower.contains(m.as_str()))
    {
        return Ok(Final::Again);
    }
    let doc = Html::parse_document(html);
    if Form::by_selector(&doc, &rules.captcha_form_selector)?.is_some() {
        return Ok(Final::Again);
    }
    Err(HostError::Changed(
        "página final do Rapidgator sem o link".into(),
    ))
}

/// Link final por um dos padrões das regras (http/https só).
fn final_link(
    html: &str,
    page_url: &Url,
    rules: &RapidgatorRules,
) -> Result<Option<Url>, HostError> {
    for pattern in &rules.final_link_patterns {
        if let Some(link) = regex(pattern)?
            .captures(html)
            .and_then(|c| page_url.join(c.get(1)?.as_str().trim()).ok())
            .filter(|u| matches!(u.scheme(), "http" | "https"))
        {
            return Ok(Some(link));
        }
    }
    Ok(None)
}

#[cfg(test)]
#[path = "parse_tests.rs"]
mod tests;
