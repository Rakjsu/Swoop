//! Proteções do painel: token por execução e guarda de `Host`/`Origin`.

use crate::Shared;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

/// Política de conteúdo das páginas servidas: nada de fora, nada inline.
const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self'; \
    img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'";

/// Token de 256 bits, em hexadecimal. Vive só na memória do processo.
#[derive(Clone)]
pub struct Token(String);

impl Token {
    /// Sorteia um token novo com o gerador do sistema.
    pub fn generate() -> Result<Self, getrandom::Error> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes)?;
        Ok(Self(hex::encode(bytes)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Compara sem atalho: o tempo não depende de onde a diferença está.
    pub fn matches(&self, candidate: &str) -> bool {
        let (a, b) = (self.0.as_bytes(), candidate.as_bytes());
        a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
    }
}

impl std::fmt::Debug for Token {
    /// Nunca imprime o valor (logs, pânicos).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token(…)")
    }
}

/// Em toda requisição: `Host` e `Origin` precisam ser 127.0.0.1/localhost
/// na porta do painel. Também põe os cabeçalhos de segurança na resposta.
pub async fn origin(State(shared): State<Arc<Shared>>, req: Request, next: Next) -> Response {
    if !host_ok(req.headers(), shared.port) || !origin_ok(req.headers(), shared.port) {
        return (StatusCode::FORBIDDEN, "origem recusada").into_response();
    }
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CSP),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    res
}

/// Rotas da API: `Authorization: Bearer <token>`.
pub async fn bearer(State(shared): State<Arc<Shared>>, req: Request, next: Next) -> Response {
    let ok = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| shared.token.matches(t));
    if !ok {
        return (StatusCode::UNAUTHORIZED, "token ausente ou inválido").into_response();
    }
    next.run(req).await
}

/// Nomes aceitos para o painel nesta porta.
fn allowed_hosts(port: u16) -> [String; 2] {
    [format!("127.0.0.1:{port}"), format!("localhost:{port}")]
}

fn host_ok(headers: &HeaderMap, port: u16) -> bool {
    headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|h| {
            allowed_hosts(port)
                .iter()
                .any(|a| a.eq_ignore_ascii_case(h))
        })
}

/// Sem `Origin` (navegação direta, ferramentas) passa; com, tem de ser a nossa.
fn origin_ok(headers: &HeaderMap, port: u16) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    allowed_hosts(port)
        .iter()
        .any(|h| origin.eq_ignore_ascii_case(&format!("http://{h}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_compara_inteiro() {
        let t = Token::generate().unwrap();
        assert_eq!(t.as_str().len(), 64);
        assert!(t.matches(t.as_str()));
        assert!(!t.matches(&t.as_str()[..63]));
        assert!(!t.matches(""));
        assert_eq!(format!("{t:?}"), "Token(…)");
    }

    #[test]
    fn host_e_origin_so_da_porta_local() {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, "127.0.0.1:9000".parse().unwrap());
        assert!(host_ok(&h, 9000));
        assert!(!host_ok(&h, 9001));
        h.insert(header::HOST, "evil.example:9000".parse().unwrap());
        assert!(!host_ok(&h, 9000));

        assert!(origin_ok(&HeaderMap::new(), 9000));
        h.insert(header::ORIGIN, "http://localhost:9000".parse().unwrap());
        assert!(origin_ok(&h, 9000));
        h.insert(header::ORIGIN, "http://evil.example".parse().unwrap());
        assert!(!origin_ok(&h, 9000));
    }
}
