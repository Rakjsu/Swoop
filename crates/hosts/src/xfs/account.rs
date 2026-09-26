//! XFileSharing premium, parte pura: respostas da API (com a chave), página
//! depois do login, página da conta e a página do arquivo vista por quem está
//! logado.

use super::parse::{self, Form};
use crate::page;
use crate::rules::XfsRules;
use regex::Regex;
use scraper::Html;
use serde_json::Value;
use std::time::SystemTime;
use swoop_core::{AccountInfo, HostError};
use url::Url;

/// MiB: a API do XFS informa o tráfego restante em MB.
const MIB: u64 = 1024 * 1024;

/// Número que a API manda como número ou como texto.
fn num(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str()?.trim().parse().ok())
}

/// `{"status":200,"msg":"OK","result":…}` → `result`, ou o erro da conta.
fn api_result(json: &str, host: &str) -> Result<Value, HostError> {
    let v: Value = serde_json::from_str(json)
        .map_err(|_| HostError::Changed(format!("a API do {host} respondeu fora do formato")))?;
    let msg = v.get("msg").and_then(Value::as_str).unwrap_or("sem motivo");
    match v.get("status").and_then(num) {
        Some(200) => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
        Some(404) => Err(HostError::Offline),
        _ => Err(HostError::Account(format!("{host} recusou a conta: {msg}"))),
    }
}

/// Link direto pela API (`api_direct_link`).
pub fn api_link(json: &str, host: &str, base: &Url) -> Result<Url, HostError> {
    let r = api_result(json, host)?;
    let url = r
        .get("url")
        .and_then(Value::as_str)
        .or_else(|| r.as_str())
        .ok_or_else(|| HostError::Changed(format!("a API do {host} não mandou o link")))?;
    base.join(url.trim())
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https"))
        .ok_or_else(|| HostError::Changed(format!("a API do {host} mandou um link inválido")))
}

/// Conta pela API (`api_account_info`).
pub fn api_info(json: &str, host: &str, now_ms: i64) -> Result<AccountInfo, HostError> {
    let r = api_result(json, host)?;
    let until = r
        .get("premium_expire")
        .and_then(Value::as_str)
        .and_then(datetime_ms);
    let flag = r.get("premium").and_then(num).map(|p| p > 0);
    Ok(AccountInfo {
        premium: until.map_or(flag.unwrap_or(false), |u| u > now_ms),
        premium_until_ms: until,
        traffic_left: r
            .get("traffic_left")
            .and_then(num)
            .filter(|t| *t >= 0)
            .map(|t| t as u64 * MIB),
    })
}

/// Página depois do `op=login`: entrou, ou a conta foi recusada.
pub fn login_result(html: &str, rules: &XfsRules, host: &str) -> Result<(), HostError> {
    let lower = html.to_lowercase();
    let has = |m: &[String]| m.iter().any(|x| lower.contains(x.as_str()));
    if has(&rules.bad_login_markers) {
        return Err(HostError::Account(format!(
            "{host} recusou o usuário ou a senha"
        )));
    }
    if has(&rules.logged_in_markers) {
        return Ok(());
    }
    Err(HostError::Changed(format!(
        "o login do {host} não confirmou a entrada"
    )))
}

/// Página da conta (`account_page`): fim do premium.
pub fn page_info(html: &str, rules: &XfsRules, now_ms: i64) -> AccountInfo {
    let until = Regex::new(&rules.premium_until_pattern)
        .ok()
        .and_then(|re| datetime_ms(re.captures(html)?.get(1)?.as_str()));
    AccountInfo {
        premium: until.is_some_and(|u| u > now_ms),
        premium_until_ms: until,
        traffic_left: None,
    }
}

/// O que a página do arquivo mostra a quem está logado com premium.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Premium {
    Link(Url),
    /// Formulário do download premium (enviar e ler o link).
    Form(Form),
}

/// Página do arquivo com a sessão premium: link, formulário premium ou o
/// fluxo grátis (= a conta não é premium).
pub fn premium_page(
    html: &str,
    page_url: &Url,
    rules: &XfsRules,
    host: &str,
    now: SystemTime,
) -> Result<Premium, HostError> {
    match parse::blocked(html, rules, now) {
        Err(HostError::PremiumOnly | HostError::Wait { .. }) => return Err(not_premium(host)),
        other => other?,
    }
    let doc = Html::parse_document(html);
    if let Some(href) = page::attr(&doc, &rules.direct_link_selector, "href")?
        && let Ok(link) = page_url.join(href.trim())
        && matches!(link.scheme(), "http" | "https")
    {
        return Ok(Premium::Link(link));
    }
    // O `download2` do grátis leva `method_free` preenchido (e contador).
    if let Some(form) = parse::form_with_op(&doc, &rules.download2_op)?
        && !form
            .fields
            .iter()
            .any(|(k, v)| k == &rules.free_field && !v.is_empty())
    {
        return Ok(Premium::Form(form));
    }
    // Alguns sites mostram a escolha grátis/premium: vale o botão premium.
    if let Some(mut form) = parse::form_with_op(&doc, &rules.download1_op)?
        && form.fields.iter().any(|(k, _)| k == "method_premium")
    {
        form.fields.retain(|(k, _)| k != &rules.free_field);
        return Ok(Premium::Form(form));
    }
    Err(not_premium(host))
}

fn not_premium(host: &str) -> HostError {
    HostError::Account(format!("a conta do {host} não é premium (ou venceu)"))
}

/// "2027-01-31 12:00:00" (ou só a data) em UTC → ms Unix.
pub fn datetime_ms(s: &str) -> Option<i64> {
    let s = s.trim();
    let (date, time) = s.split_once([' ', 'T']).unwrap_or((s, "00:00:00"));
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) {
        return None;
    }
    let mut t = time.split(':').map(|p| p.parse::<i64>().ok());
    let (h, min, sec) = (
        t.next().flatten().unwrap_or(0),
        t.next().flatten().unwrap_or(0),
        t.next().flatten().unwrap_or(0),
    );
    // Dias desde 1970-01-01 (algoritmo "days from civil", de Howard Hinnant).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 24 + h) * 60 + min) * 60_000 + sec * 1000)
}

#[cfg(test)]
#[path = "account_tests.rs"]
mod tests;
