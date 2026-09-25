//! Busca de páginas/APIs dos servidores e leitura de HTML por seletor CSS.
//! Erros de rede nunca carregam a URL (pode ter chave no link).

use crate::plugin::HostCtx;
use scraper::{Html, Selector};
use swoop_core::HostError;
use swoop_net::reqwest::{self, Response, header};
use url::Url;

/// Maior página/resposta de API que um plugin lê (o resto é arquivo).
const MAX_PAGE: usize = 4 * 1024 * 1024;

/// Resposta de uma página ou API, já lida.
#[derive(Debug, Clone)]
pub struct Page {
    pub status: u16,
    /// Endereço final, depois dos redirecionamentos.
    pub url: Url,
    pub body: String,
    /// O endereço entregou o próprio arquivo (corpo não lido): `url` já é
    /// o link direto.
    pub file: bool,
}

/// GET simples pelo cliente de páginas.
pub async fn get(ctx: &HostCtx, url: &Url) -> Result<Page, HostError> {
    let res = ctx.http.get(url.clone()).send().await.map_err(network)?;
    into_page(ctx, res).await
}

/// POST de formulário pelo cliente de páginas. Se a resposta for o próprio
/// arquivo (redirecionamento para o link direto), `url` já é o link.
pub async fn post_form(
    ctx: &HostCtx,
    url: &Url,
    fields: &[(String, String)],
) -> Result<Page, HostError> {
    let res = ctx
        .http
        .post(url.clone())
        .header(header::REFERER, url.as_str())
        .form(fields)
        .send()
        .await
        .map_err(network)?;
    into_page(ctx, res).await
}

/// Lê a resposta: arquivo (sem ler o corpo) ou página (até `MAX_PAGE`).
async fn into_page(ctx: &HostCtx, res: Response) -> Result<Page, HostError> {
    let status = res.status().as_u16();
    let url = res.url().clone();
    if is_file(res.headers()) {
        return Ok(Page {
            status,
            url,
            body: String::new(),
            file: true,
        });
    }
    let body = read_body(res).await?;
    ctx.record(&url, status, &body);
    Ok(Page {
        status,
        url,
        body,
        file: false,
    })
}

/// A resposta é um arquivo (anexo ou conteúdo que não é texto)?
pub fn is_file(h: &header::HeaderMap) -> bool {
    let text = |name| {
        h.get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_ascii_lowercase)
    };
    let attachment =
        text(header::CONTENT_DISPOSITION).is_some_and(|v| v.trim_start().starts_with("attachment"));
    let binary = text(header::CONTENT_TYPE).is_some_and(|v| {
        !(v.starts_with("text/") || ["json", "xml", "javascript"].iter().any(|t| v.contains(t)))
    });
    attachment || binary
}

/// Corpo como texto, até `MAX_PAGE` (mais que isso não é página).
pub async fn read_body(mut res: Response) -> Result<String, HostError> {
    let mut buf = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(network)? {
        buf.extend_from_slice(&chunk);
        if buf.len() > MAX_PAGE {
            return Err(HostError::Changed(
                "resposta grande demais para ser uma página".into(),
            ));
        }
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Erro de rede sem a URL, com a causa de fundo (conexão recusada, tempo
/// esgotado, certificado…), que não carrega o endereço.
pub fn network(e: reqwest::Error) -> HostError {
    // Só as causas internas: o próprio erro do reqwest traz a URL.
    let mut cause = None;
    let mut next = std::error::Error::source(&e);
    while let Some(inner) = next {
        cause = Some(inner.to_string());
        next = inner.source();
    }
    let top = e.without_url().to_string();
    match cause {
        Some(c) if c != top => HostError::Network(format!("{top}: {c}")),
        _ => HostError::Network(top),
    }
}

/// Seletor das regras; inválido = regra quebrada (mensagem clara).
pub fn selector(css: &str) -> Result<Selector, HostError> {
    Selector::parse(css)
        .map_err(|_| HostError::Changed(format!("seletor inválido nas regras: {css}")))
}

/// Atributo do primeiro elemento que casa com o seletor.
pub fn attr(doc: &Html, css: &str, name: &str) -> Result<Option<String>, HostError> {
    let sel = selector(css)?;
    Ok(doc
        .select(&sel)
        .find_map(|el| el.value().attr(name))
        .map(str::to_owned))
}

/// Texto do primeiro elemento que casa, sem espaços nas pontas.
pub fn text(doc: &Html, css: &str) -> Result<Option<String>, HostError> {
    let sel = selector(css)?;
    Ok(doc
        .select(&sel)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_owned())
        .filter(|t| !t.is_empty()))
}

/// Tamanho como os sites escrevem ("12.3MB", "1,2 GB", "845 KB") → bytes.
pub fn human_size(s: &str) -> Option<u64> {
    let s = s.trim().replace(',', ".");
    let split = s.find(|c: char| c.is_ascii_alphabetic())?;
    let (num, unit) = s.split_at(split);
    let n: f64 = num.trim().parse().ok()?;
    let mult = match unit.trim().to_ascii_uppercase().as_str() {
        "B" => 1.0,
        "K" | "KB" | "KIB" => 1024.0,
        "M" | "MB" | "MIB" => 1024.0 * 1024.0,
        "G" | "GB" | "GIB" => 1024.0 * 1024.0 * 1024.0,
        "T" | "TB" | "TIB" => 1024.0_f64.powi(4),
        _ => return None,
    };
    Some((n * mult).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tamanhos_escritos_pelos_sites() {
        assert_eq!(human_size("845 KB"), Some(845 * 1024));
        assert_eq!(human_size("12.5MB"), Some(13_107_200));
        assert_eq!(human_size("1,5 GB"), Some(1_610_612_736));
        assert_eq!(human_size("1.2G"), Some(1_288_490_189));
        assert_eq!(human_size("abc"), None);
    }

    #[test]
    fn anexo_ou_binario_e_arquivo_nao_pagina() {
        let h = |pairs: &[(header::HeaderName, &str)]| {
            let mut m = header::HeaderMap::new();
            for (k, v) in pairs {
                m.insert(k.clone(), v.parse().unwrap());
            }
            m
        };
        assert!(!is_file(&h(&[(
            header::CONTENT_TYPE,
            "text/html; charset=utf-8"
        )])));
        assert!(!is_file(&h(&[(header::CONTENT_TYPE, "application/json")])));
        assert!(!is_file(&h(&[])));
        assert!(is_file(&h(&[(
            header::CONTENT_TYPE,
            "application/octet-stream"
        )])));
        assert!(is_file(&h(&[(header::CONTENT_TYPE, "application/zip")])));
        assert!(is_file(&h(&[
            (header::CONTENT_TYPE, "text/plain"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"a.txt\""
            ),
        ])));
    }

    #[test]
    fn seletor_invalido_vira_erro_de_regra() {
        let doc = Html::parse_document("<a id='x' href='/y'>ok</a>");
        assert_eq!(attr(&doc, "a#x", "href").unwrap().as_deref(), Some("/y"));
        assert_eq!(text(&doc, "a#x").unwrap().as_deref(), Some("ok"));
        assert!(matches!(
            attr(&doc, "a[[", "href"),
            Err(HostError::Changed(_))
        ));
    }
}
