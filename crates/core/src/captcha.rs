//! Captcha pedido por um servidor. O Swoop nunca resolve sozinho: guarda o
//! desafio, o usuário resolve na janela do app e o token volta para o plugin,
//! que envia o formulário (só depois do contador do site).

use serde::{Deserialize, Serialize};
use url::Url;

/// Tipo de captcha (define o widget que a janela mostra).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CaptchaKind {
    Recaptcha2 {
        site_key: String,
    },
    Hcaptcha {
        site_key: String,
    },
    Turnstile {
        site_key: String,
    },
    /// Imagem com um código (captcha próprio do site).
    Image {
        url: Url,
    },
}

/// O que é preciso para mostrar o captcha e depois enviar a resposta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptchaChallenge {
    pub kind: CaptchaKind,
    /// Página do site: a janela abre nesta origem (as chaves de captcha só
    /// valem no domínio delas).
    pub page_url: Url,
    /// Formulário que leva a resposta: endereço e campos ocultos.
    pub action: Url,
    pub fields: Vec<(String, String)>,
    /// Campo da resposta (`g-recaptcha-response`, `code`…).
    pub answer_field: String,
    /// Não enviar antes disto (fim do contador do site), ms Unix.
    pub not_before_ms: i64,
}

impl CaptchaChallenge {
    /// Servidor do captcha, para mostrar na interface.
    pub fn host(&self) -> &str {
        self.page_url.host_str().unwrap_or("?")
    }
}

/// Resposta do usuário a um desafio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptchaAnswer {
    pub challenge: CaptchaChallenge,
    pub token: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desafio_vai_e_volta_em_json() {
        let c = CaptchaChallenge {
            kind: CaptchaKind::Recaptcha2 {
                site_key: "6Lc-chave".into(),
            },
            page_url: Url::parse("https://fastfile.cc/abc123def456").unwrap(),
            action: Url::parse("https://fastfile.cc/abc123def456").unwrap(),
            fields: vec![("op".into(), "download2".into())],
            answer_field: "g-recaptcha-response".into(),
            not_before_ms: 1_000,
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"type\":\"recaptcha2\""));
        assert_eq!(serde_json::from_str::<CaptchaChallenge>(&json).unwrap(), c);
        assert_eq!(c.host(), "fastfile.cc");
    }
}
