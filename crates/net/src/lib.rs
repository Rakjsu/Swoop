//! Rede do Swoop: fábrica do cliente HTTP, parsers de cabeçalhos e URL
//! redigida para logs. Os parsers são puros e testados sem rede.

pub mod client;
pub mod headers;
pub mod redact;

pub use client::{DEFAULT_USER_AGENT, download_client, install_crypto_provider, page_client};
pub use headers::{
    ContentRange, parse_content_disposition, parse_content_range, parse_retry_after,
};
pub use redact::Redacted;
pub use reqwest;
