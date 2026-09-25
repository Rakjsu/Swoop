//! Sonda: um `GET` com `Range: bytes=0-0` descobre tamanho, suporte a Range,
//! validadores (ETag/Last-Modified) e o nome do Content-Disposition, sem
//! baixar o arquivo.

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

/// Faz a sonda. Erro HTTP/rede vira `HttpFailure` para o resolvedor classificar.
pub async fn probe(
    client: &swoop_net::reqwest::Client,
    resolved: &Resolved,
) -> Result<Probe, HttpFailure> {
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
        },
        _ => return Err(request::status_failure(status, headers, SystemTime::now())),
    };
    if status == StatusCode::PARTIAL_CONTENT {
        // 1 byte: ler devolve a conexão ao pool para as conexões de verdade.
        let _ = resp.bytes().await;
    } else {
        // Resposta inteira (sem Range): descartar fecha a conexão sem baixar.
        drop(resp);
    }
    Ok(probe)
}
