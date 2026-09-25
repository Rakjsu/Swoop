//! Fábrica do cliente HTTP usado pelo motor.
//!
//! - HTTP/1.1 apenas: com HTTP/2 as N conexões de um download viram um só
//!   socket multiplexado e a segmentação perde o sentido.
//! - Sem descompressão automática: o motor pede `Accept-Encoding: identity`,
//!   porque gzip faria os offsets de Range se referirem a bytes comprimidos.
//! - TLS via rustls com o provedor `ring` (sem aws-lc, que exige NASM no Windows)
//!   e certificados do sistema (rustls-platform-verifier).

use reqwest::cookie::Jar;
use std::sync::Arc;
use std::time::Duration;

/// User-Agent padrão para links diretos. Plugins que imitam o navegador
/// (fase 3+) sobrescrevem por requisição.
pub const DEFAULT_USER_AGENT: &str = concat!("Swoop/", env!("CARGO_PKG_VERSION"));

/// Registra o `ring` como provedor de criptografia do rustls no processo.
///
/// Precisa rodar no início de todo `main` antes de qualquer conexão TLS.
/// Chamar de novo é inofensivo (o segundo registro é ignorado).
pub fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Cliente para as páginas e APIs dos servidores (plugins): cookies da
/// sessão no `jar` (alguns fluxos dependem deles, e o plugin pode repassá-los
/// ao motor), UA de navegador e tempo total limitado, porque uma página nunca
/// é grande.
pub fn page_client(user_agent: &str, jar: Arc<Jar>) -> Result<reqwest::Client, reqwest::Error> {
    install_crypto_provider();
    reqwest::Client::builder()
        .user_agent(user_agent)
        .cookie_provider(jar)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30))
        .build()
}

/// Cliente para baixar arquivos (sonda e segmentos).
pub fn download_client(user_agent: &str) -> Result<reqwest::Client, reqwest::Error> {
    install_crypto_provider();
    reqwest::Client::builder()
        .user_agent(user_agent)
        .http1_only()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .pool_idle_timeout(Duration::from_secs(30))
        .tcp_nodelay(true)
        .build()
}
