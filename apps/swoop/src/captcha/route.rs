//! Para onde a janela de captcha pode navegar (função pura, testada).
//!
//! Só o site que pediu o captcha (e os subdomínios dele) e os provedores de
//! captcha abrem; o endereço reservado `swoop-captcha.invalid` devolve a
//! resposta ao app; o resto é negado.

use tauri::Url;

/// Host reservado (`.invalid` nunca existe no DNS) para devolver a resposta.
pub const DONE_HOST: &str = "swoop-captcha.invalid";

/// Provedores de captcha: (domínio, prefixo de caminho).
const PROVIDERS: &[(&str, &str)] = &[
    ("google.com", "/recaptcha/"),
    ("recaptcha.net", "/recaptcha/"),
    ("gstatic.com", "/"),
    ("hcaptcha.com", "/"),
    ("challenges.cloudflare.com", "/"),
];

/// O que fazer com uma navegação.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Allow,
    /// O usuário resolveu: a resposta vai para o serviço e a janela fecha.
    Token(String),
    /// O usuário desistiu: a janela fecha e o download continua esperando.
    Close,
    Deny,
}

/// Decide a navegação `url` da janela do download `id`, aberta em `site`.
pub fn route(url: &Url, site: &Url, id: i64) -> Route {
    if url.host_str() == Some(DONE_HOST) {
        return answer(url, id);
    }
    match url.scheme() {
        // iframes vazios e `srcdoc` (captcha de dígitos)
        "about" => return Route::Allow,
        "https" | "http" => {}
        _ => return Route::Deny,
    }
    let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
        return Route::Deny;
    };
    if !url.username().is_empty() || url.password().is_some() {
        return Route::Deny;
    }
    let https = url.scheme() == "https";
    if same_site(&host, site) && (https || url.scheme() == site.scheme()) {
        return Route::Allow;
    }
    let provider = PROVIDERS
        .iter()
        .any(|(d, path)| within(&host, d) && url.path().starts_with(path));
    if https && provider {
        Route::Allow
    } else {
        Route::Deny
    }
}

/// `https://swoop-captcha.invalid/done?id=…&token=…` ou `/cancel?id=…`.
fn answer(url: &Url, id: i64) -> Route {
    if url.scheme() != "https" || url.port().is_some() || !url.username().is_empty() {
        return Route::Deny;
    }
    let mut got_id = None;
    let mut token = None;
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "id" => got_id = v.parse::<i64>().ok(),
            "token" => token = Some(v.into_owned()),
            _ => {}
        }
    }
    if got_id != Some(id) {
        return Route::Deny;
    }
    match url.path() {
        "/done" => token
            .filter(|t| !t.trim().is_empty())
            .map_or(Route::Deny, Route::Token),
        "/cancel" => Route::Close,
        _ => Route::Deny,
    }
}

/// O host é o do site (sem `www.`) ou um subdomínio dele.
fn same_site(host: &str, site: &Url) -> bool {
    site.host_str()
        .map(|h| h.trim_start_matches("www.").to_ascii_lowercase())
        .is_some_and(|base| within(host, &base))
}

/// `host` é `domain` ou termina em `.domain`.
fn within(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|rest| rest.ends_with('.'))
}

#[cfg(test)]
#[path = "route_tests.rs"]
mod tests;
