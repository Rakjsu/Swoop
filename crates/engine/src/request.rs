//! Montagem das requisições e conversão de falhas para `HttpFailure`.

use std::time::SystemTime;
use swoop_core::{HttpFailure, Resolved};
use swoop_net::reqwest::{self, StatusCode, header};

/// GET da URL resolvida com os cabeçalhos do plugin e sem compressão
/// (offsets de Range precisam ser do arquivo, não do gzip).
pub fn base(client: &reqwest::Client, resolved: &Resolved) -> reqwest::RequestBuilder {
    let mut req = client
        .get(resolved.url.clone())
        .header(header::ACCEPT_ENCODING, "identity");
    for (k, v) in &resolved.headers {
        req = req.header(k.as_str(), v.as_str());
    }
    req
}

/// Falha sem resposta HTTP (DNS, conexão, timeout, queda).
pub fn network_failure(e: &reqwest::Error) -> HttpFailure {
    HttpFailure {
        status: e.status().map(|s| s.as_u16()),
        retry_after: None,
        network: Some(describe(e)),
    }
}

/// Falha com status HTTP (lê `Retry-After`).
pub fn status_failure(
    status: StatusCode,
    headers: &header::HeaderMap,
    now: SystemTime,
) -> HttpFailure {
    HttpFailure {
        status: Some(status.as_u16()),
        retry_after: headers
            .get(header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| swoop_net::parse_retry_after(v, now)),
        network: None,
    }
}

/// Texto curto do erro de rede, sem a URL (pode ter token na query).
pub fn describe(e: &reqwest::Error) -> String {
    let kind = if e.is_timeout() {
        "tempo esgotado"
    } else if e.is_connect() {
        "falha ao conectar"
    } else if e.is_body() || e.is_decode() {
        "conexão interrompida"
    } else {
        "falha de rede"
    };
    let mut source = std::error::Error::source(e);
    let mut detail = String::new();
    while let Some(s) = source {
        detail = s.to_string();
        source = s.source();
    }
    if detail.is_empty() {
        kind.to_owned()
    } else {
        format!("{kind}: {detail}")
    }
}

/// Mensagem para o usuário a partir de uma `HttpFailure`.
pub fn failure_message(f: &HttpFailure) -> String {
    match (&f.network, f.status) {
        (Some(net), _) => net.clone(),
        (None, Some(s)) => format!("o servidor respondeu HTTP {s}"),
        (None, None) => "falha desconhecida".to_owned(),
    }
}
