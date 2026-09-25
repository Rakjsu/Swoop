//! Link direto: a própria URL é o arquivo. A sonda do motor descobre tamanho,
//! nome e suporte a Range; `check` faz o mesmo para o coletor pedindo só o
//! primeiro byte.

use crate::page::network;
use crate::plugin::FileInfo;
use async_trait::async_trait;
use swoop_core::filename::file_name_from_url;
use swoop_core::{HostError, ResolveRequest, Resolved, Resolver};
use swoop_net::reqwest::{self, header};
use swoop_net::{parse_content_disposition, parse_content_range};
use url::Url;

/// Resolve links `http`/`https` sem transformação.
#[derive(Debug, Default, Clone, Copy)]
pub struct DirectResolver;

#[async_trait]
impl Resolver for DirectResolver {
    async fn resolve(&self, req: &ResolveRequest) -> Result<Resolved, HostError> {
        match req.url.scheme() {
            "http" | "https" => Ok(Resolved::direct(req.url.clone())),
            _ => Err(HostError::Unsupported),
        }
    }
}

/// Nome e tamanho de um link direto (pede `bytes=0-0`; o corpo é descartado).
pub async fn check(http: &reqwest::Client, url: &Url) -> Result<FileInfo, HostError> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(HostError::Unsupported);
    }
    let res = http
        .get(url.clone())
        .header(header::RANGE, "bytes=0-0")
        .header(header::ACCEPT_ENCODING, "identity")
        .send()
        .await
        .map_err(network)?;
    let status = res.status().as_u16();
    let h = res.headers();
    let text = |name: header::HeaderName| h.get(name).and_then(|v| v.to_str().ok());
    let size = match status {
        206 => text(header::CONTENT_RANGE)
            .and_then(parse_content_range)
            .and_then(|r| r.total),
        200 => res.content_length(),
        404 | 410 => return Err(HostError::Offline),
        401 | 403 => return Err(HostError::AccessDenied),
        s => return Err(HostError::Http(s)),
    };
    let name = text(header::CONTENT_DISPOSITION)
        .and_then(parse_content_disposition)
        .or_else(|| file_name_from_url(res.url()));
    Ok(FileInfo { name, size })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(s: &str) -> ResolveRequest {
        ResolveRequest::new(Url::parse(s).unwrap())
    }

    #[tokio::test]
    async fn aceita_http_e_recusa_outros_esquemas() {
        let r = DirectResolver
            .resolve(&req("https://Exemplo.com/a.zip"))
            .await
            .unwrap();
        assert_eq!(r.host_key, "exemplo.com");
        assert!(r.resumable);
        assert_eq!(
            DirectResolver
                .resolve(&req("ftp://exemplo.com/a.zip"))
                .await
                .unwrap_err(),
            HostError::Unsupported
        );
    }
}
