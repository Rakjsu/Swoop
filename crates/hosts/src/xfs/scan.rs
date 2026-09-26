//! XFileSharing, leituras tolerantes (puras): o contador em vários formatos
//! e o link final quando a página não o traz no lugar de sempre.
//!
//! O contador nunca é pulado: sem número legível na página, vale a espera
//! padrão das regras (`fallback_countdown_secs`).

use crate::page;
use crate::rules::XfsRules;
use regex::Regex;
use scraper::Html;
use swoop_core::HostError;
use url::Url;

/// Segundos que a página manda esperar e se o número foi achado nela.
pub fn countdown(doc: &Html, rules: &XfsRules) -> Result<(u64, bool), HostError> {
    let found = page::text(doc, &rules.countdown_selector)?
        .and_then(|t| digits(&t))
        .or(page::attr(doc, "[data-seconds]", "data-seconds")?.and_then(|t| digits(&t)))
        .or_else(|| from_scripts(doc, &rules.countdown_script_pattern));
    Ok(match found {
        Some(secs) => (secs.min(rules.max_countdown_secs), true),
        None => (rules.fallback_countdown_secs, false),
    })
}

/// Só os dígitos do texto ("Wait 60 seconds" → 60).
fn digits(text: &str) -> Option<u64> {
    text.chars()
        .filter(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
}

/// Primeiro número que a regex acha nos `<script>` da página.
fn from_scripts(doc: &Html, pattern: &str) -> Option<u64> {
    let re = Regex::new(pattern).ok()?;
    let scripts = page::selector("script").ok()?;
    doc.select(&scripts)
        .find_map(|s| {
            let code = s.text().collect::<String>();
            re.captures(&code)?.get(1)?.as_str().parse().ok()
        })
        // Um 0 achado em script alheio (anúncio) não pode zerar a espera.
        .filter(|secs| *secs > 0)
}

/// Link final fora do lugar de sempre: `<meta refresh>`, redirecionamento em
/// `<script>` ou um `<a>` para um servidor de arquivos do site
/// (`s12.fastfile.cc/…/arquivo.ext`).
pub fn final_link(doc: &Html, page_url: &Url, rules: &XfsRules) -> Result<Option<Url>, HostError> {
    let web = |raw: &str| {
        page_url
            .join(raw.trim())
            .ok()
            .filter(|u| matches!(u.scheme(), "http" | "https"))
    };
    let refresh = page::selector("meta[http-equiv]")?;
    for meta in doc.select(&refresh) {
        let v = meta.value();
        if v.attr("http-equiv")
            .is_some_and(|h| h.eq_ignore_ascii_case("refresh"))
            && let Some(url) = v.attr("content").and_then(refresh_target).and_then(web)
        {
            return Ok(Some(url));
        }
    }
    if let Ok(re) = Regex::new(&rules.final_script_pattern) {
        let scripts = page::selector("script")?;
        for s in doc.select(&scripts) {
            let code = s.text().collect::<String>();
            if let Some(url) = re
                .captures(&code)
                .and_then(|c| c.get(1))
                .and_then(|m| web(m.as_str()))
            {
                return Ok(Some(url));
            }
        }
    }
    let anchors = page::selector("a[href]")?;
    Ok(doc
        .select(&anchors)
        .filter_map(|a| web(a.value().attr("href")?))
        .find(|u| file_server(u, page_url)))
}

/// Destino de um `content="5; url=…"` (sem diferenciar maiúsculas).
fn refresh_target(content: &str) -> Option<&str> {
    let at = content.to_ascii_lowercase().find("url=")?;
    Some(content[at + 4..].trim().trim_matches(['\'', '"']))
}

/// Endereço num subdomínio do site (servidor de arquivos) com cara de
/// arquivo: `/d/…` ou último trecho com extensão.
fn file_server(url: &Url, page_url: &Url) -> bool {
    let (Some(host), Some(site)) = (url.host_str(), page_url.host_str()) else {
        return false;
    };
    let site = site.trim_start_matches("www.");
    let sub = host
        .strip_suffix(site)
        .is_some_and(|rest| rest.ends_with('.') && rest != "www.");
    let last = url
        .path_segments()
        .and_then(|mut s| s.next_back())
        .unwrap_or("");
    sub && (url.path().contains("/d/") || last.contains('.'))
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;
