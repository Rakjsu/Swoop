//! Sonda: um `GET` com `Range: bytes=0-0` descobre tamanho, suporte a Range,
//! validadores (ETag/Last-Modified) e o nome do Content-Disposition, sem
//! baixar o arquivo.
//!
//! Sem Range (`RangeStyle::None`, ou servidor que respondeu 200), a resposta
//! da sonda já é o arquivo inteiro: ela segue para a transferência em vez de
//! ser descartada, e o link é pedido uma vez só (links de sessão de uso único).

use crate::request;
use std::time::SystemTime;
use swoop_core::{HttpFailure, RangeStyle, Resolved};
use swoop_net::reqwest::{StatusCode, header};

/// O que a sonda descobriu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub size: Option<u64>,
    pub range_ok: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub disposition_name: Option<String>,
    /// A resposta é uma página (HTML ou JSON sem ser anexo), não o arquivo:
    /// link expirado, limite do servidor ou site que mudou.
    pub page: bool,
}

impl Probe {
    /// Valor para `If-Range`: ETag forte, senão Last-Modified.
    pub fn validator(&self) -> Option<String> {
        self.etag
            .clone()
            .filter(|e| !e.starts_with("W/"))
            .or_else(|| self.last_modified.clone())
    }
}

/// HTML ou JSON que não vem como anexo é página ou erro do site, não arquivo
/// (`{"error":"Denied by IP"}`); um `.rar` mandado com `text/html` mas como
/// anexo continua valendo, e um `.json` esperado como JSON também.
fn is_page(content_type: Option<&str>, disposition: Option<&str>, expected: &str) -> bool {
    let essence = content_type
        .and_then(|t| t.split(';').next())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let json_file = [".json", ".jsonl", ".ndjson", ".geojson"]
        .iter()
        .any(|e| expected.to_ascii_lowercase().ends_with(e));
    let page = essence.starts_with("text/html")
        || essence.starts_with("application/xhtml")
        || (essence == "application/json" && !json_file);
    let attachment = disposition.is_some_and(|d| {
        d.trim_start()
            .to_ascii_lowercase()
            .starts_with("attachment")
    });
    page && !attachment
}

/// A resposta é uma página no lugar do arquivo esperado do `resolved`?
pub fn page_instead_of_file(headers: &header::HeaderMap, resolved: &Resolved) -> bool {
    let text = |name| headers.get(name).and_then(|v| v.to_str().ok());
    let expected = resolved
        .file_name
        .clone()
        .or_else(|| swoop_core::filename::file_name_from_url(&resolved.url))
        .unwrap_or_default();
    is_page(
        text(header::CONTENT_TYPE),
        text(header::CONTENT_DISPOSITION),
        &expected,
    )
}

/// Resposta inteira (200) guardada para a transferência sem Range.
pub type Body = swoop_net::reqwest::Response;

/// Faz a sonda. Erro HTTP/rede vira `HttpFailure` para o resolvedor classificar.
/// Com 200 de um arquivo, devolve também a resposta, que já é o download.
pub async fn probe(
    client: &swoop_net::reqwest::Client,
    resolved: &Resolved,
) -> Result<(Probe, Option<Body>), HttpFailure> {
    let mut req = request::base(client, resolved);
    if resolved.range != RangeStyle::None {
        req = req.header(header::RANGE, "bytes=0-0");
    }
    let resp = req.send().await.map_err(|e| request::network_failure(&e))?;
    let status = resp.status();
    let headers = resp.headers();
    let text = |name: header::HeaderName| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    let disposition_name =
        text(header::CONTENT_DISPOSITION).and_then(|v| swoop_net::parse_content_disposition(&v));
    let etag = text(header::ETAG);
    let last_modified = text(header::LAST_MODIFIED);
    let page = page_instead_of_file(headers, resolved);

    let probe = match status {
        StatusCode::PARTIAL_CONTENT => {
            let range =
                text(header::CONTENT_RANGE).and_then(|v| swoop_net::parse_content_range(&v));
            Probe {
                size: range.and_then(|r| r.total),
                range_ok: range.is_some_and(|r| r.start == 0 && r.total.is_some()),
                etag,
                last_modified,
                disposition_name,
                page,
            }
        }
        StatusCode::OK => Probe {
            size: resp.content_length().filter(|_| {
                // Com Content-Encoding o tamanho não é o do arquivo.
                headers.get(header::CONTENT_ENCODING).is_none()
            }),
            range_ok: false,
            etag,
            last_modified,
            disposition_name,
            page,
        },
        _ => return Err(request::status_failure(status, headers, SystemTime::now())),
    };
    if status == StatusCode::PARTIAL_CONTENT {
        // 1 byte: ler devolve a conexão ao pool para as conexões de verdade.
        let _ = resp.bytes().await;
        return Ok((probe, None));
    }
    // Página no lugar do arquivo: descartar fecha a conexão sem ler.
    let body = (!probe.page).then_some(resp);
    Ok((probe, body))
}
