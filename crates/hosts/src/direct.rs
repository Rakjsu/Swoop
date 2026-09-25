//! Link direto: a própria URL é o arquivo. A sonda do motor descobre tamanho,
//! nome e suporte a Range.

use async_trait::async_trait;
use swoop_core::{HostError, ResolveRequest, Resolved, Resolver};

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

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

    fn req(s: &str) -> ResolveRequest {
        ResolveRequest {
            url: Url::parse(s).unwrap(),
            attempt: 0,
        }
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
