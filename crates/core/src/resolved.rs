//! O que um plugin de servidor entrega ao motor: o link direto e como baixá-lo.

use crate::captcha::CaptchaAnswer;
use std::time::SystemTime;
use url::Url;

/// Pedido de resolução de um link original (página do servidor ou link direto).
#[derive(Debug, Clone)]
pub struct ResolveRequest {
    pub url: Url,
    /// 0 na primeira vez; cresce a cada nova resolução do mesmo download.
    pub attempt: u32,
    /// Captcha resolvido pelo usuário para o desafio que o plugin pediu.
    pub captcha: Option<CaptchaAnswer>,
}

impl ResolveRequest {
    /// Primeira resolução de um link, sem captcha.
    pub fn new(url: Url) -> Self {
        Self {
            url,
            attempt: 0,
            captcha: None,
        }
    }
}

/// Como o servidor aceita pedidos parciais.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeStyle {
    /// Descobrir com uma sonda `Range: bytes=0-0`.
    Probe,
    /// Sabidamente sem suporte: uma conexão e sem retomada.
    None,
}

/// Hash anunciado pelo servidor para conferir o arquivo no fim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Integrity {
    /// Hex minúsculo.
    Sha256(String),
    /// Hex minúsculo.
    Md5(String),
}

/// Link direto pronto para o motor. Nunca é persistido: URLs expiram, então
/// a retomada sempre resolve de novo.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub url: Url,
    /// Cabeçalhos extras exigidos pelo servidor (Referer, Authorization…).
    pub headers: Vec<(String, String)>,
    /// Nome dado pelo servidor; tem prioridade sobre Content-Disposition e URL.
    pub file_name: Option<String>,
    pub size: Option<u64>,
    pub integrity: Option<Integrity>,
    pub range: RangeStyle,
    /// Teto de conexões imposto pelo servidor/conta para este arquivo.
    pub max_connections: u16,
    /// O servidor permite continuar de onde parou.
    pub resumable: bool,
    pub expires_at: Option<SystemTime>,
    /// Agrupa limites e esperas por servidor (normalmente o host).
    pub host_key: String,
}

impl Resolved {
    /// Link direto sem restrições conhecidas (plugin `direct`).
    pub fn direct(url: Url) -> Self {
        let host_key = url
            .host_str()
            .unwrap_or("desconhecido")
            .to_ascii_lowercase();
        Self {
            url,
            headers: Vec::new(),
            file_name: None,
            size: None,
            integrity: None,
            range: RangeStyle::Probe,
            max_connections: u16::MAX,
            resumable: true,
            expires_at: None,
            host_key,
        }
    }
}
