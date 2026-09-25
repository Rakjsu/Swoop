//! Erros de servidor e a regra padrão para decidir o que fazer com uma falha HTTP.

use crate::captcha::CaptchaChallenge;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

/// Motivo de uma espera (mostrado na interface com contagem regressiva).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitReason {
    /// Contador da página do servidor (XFS etc.).
    Countdown,
    /// Limite de downloads/conexões do servidor.
    HostLimit,
    /// Cota de tráfego esgotada.
    Quota,
    /// Nova tentativa depois de erro temporário.
    Backoff,
    /// Fora da janela do agendador.
    Schedule,
}

impl WaitReason {
    /// Explicação curta para a interface.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Countdown => "contador do servidor",
            Self::HostLimit => "limite do servidor",
            Self::Quota => "cota do servidor esgotada (muitos downloads recentes)",
            Self::Backoff => "nova tentativa depois de um erro",
            Self::Schedule => "fora do horário do agendador",
        }
    }

    /// Nome estável usado no banco.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Countdown => "countdown",
            Self::HostLimit => "host_limit",
            Self::Quota => "quota",
            Self::Backoff => "backoff",
            Self::Schedule => "schedule",
        }
    }

    /// Lê o nome gravado no banco.
    pub fn parse(s: &str) -> Option<Self> {
        [
            Self::Countdown,
            Self::HostLimit,
            Self::Quota,
            Self::Backoff,
            Self::Schedule,
        ]
        .into_iter()
        .find(|r| r.as_str() == s)
    }
}

/// Falha ao resolver um link.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HostError {
    #[error("arquivo offline ou removido")]
    Offline,
    #[error("o servidor pediu uma espera")]
    Wait {
        until: SystemTime,
        reason: WaitReason,
    },
    #[error("o servidor mudou e o plugin precisa de atualização: {0}")]
    Changed(String),
    #[error("falha de rede: {0}")]
    Network(String),
    #[error("o servidor respondeu HTTP {0}")]
    Http(u16),
    #[error("link não suportado")]
    Unsupported,
    /// O servidor quer o navegador (captcha, aviso de arquivo perigoso,
    /// senha). Nunca é contornado: na fase 4 abre a janela do app para o
    /// usuário decidir. A mensagem já diz o que houve.
    #[error("{0}")]
    BrowserRequired(String),
    #[error("o arquivo é privado ou exige login")]
    AccessDenied,
    /// Só contas premium baixam este arquivo (contas entram na v0.7.0).
    #[error("o servidor só libera este arquivo para contas premium")]
    PremiumOnly,
    /// O servidor pediu um captcha que o usuário resolve na janela do app.
    #[error("o servidor pediu um captcha")]
    Captcha(Box<CaptchaChallenge>),
}

/// Falha de uma requisição durante o download (dados mínimos para classificar).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HttpFailure {
    /// Status HTTP, se houve resposta.
    pub status: Option<u16>,
    /// Valor de `Retry-After` já convertido.
    pub retry_after: Option<Duration>,
    /// Descrição do erro de rede, se não houve resposta.
    pub network: Option<String>,
}

/// O que o motor deve fazer depois de uma falha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorClass {
    /// Tentar de novo a mesma URL (com backoff; `after` sugerido pelo servidor).
    Retry { after: Option<Duration> },
    /// A URL expirou ou foi negada: pedir outra ao resolvedor.
    Reresolve,
    /// Parar até `until` e voltar para a fila.
    Wait {
        until: SystemTime,
        reason: WaitReason,
    },
    /// Sem recuperação automática.
    Fatal(String),
}

/// Regra genérica por status HTTP; plugins podem refinar via `Resolver::classify`.
pub fn default_classify(f: &HttpFailure) -> ErrorClass {
    let Some(status) = f.status else {
        return ErrorClass::Retry { after: None };
    };
    match status {
        408 | 425 | 429 | 500..=599 => ErrorClass::Retry {
            after: f.retry_after,
        },
        401 | 403 | 410 => ErrorClass::Reresolve,
        404 => ErrorClass::Fatal("arquivo não encontrado no servidor (404)".into()),
        416 => ErrorClass::Fatal("o servidor recusou a faixa pedida (416)".into()),
        s => ErrorClass::Fatal(format!("o servidor respondeu HTTP {s}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(s: u16) -> HttpFailure {
        HttpFailure {
            status: Some(s),
            ..Default::default()
        }
    }

    #[test]
    fn classifica_por_status() {
        assert_eq!(
            default_classify(&HttpFailure::default()),
            ErrorClass::Retry { after: None }
        );
        assert_eq!(
            default_classify(&status(503)),
            ErrorClass::Retry { after: None }
        );
        assert_eq!(default_classify(&status(403)), ErrorClass::Reresolve);
        assert!(matches!(
            default_classify(&status(404)),
            ErrorClass::Fatal(_)
        ));
        assert!(matches!(
            default_classify(&status(400)),
            ErrorClass::Fatal(_)
        ));
    }

    #[test]
    fn retry_after_passa_adiante() {
        let f = HttpFailure {
            status: Some(429),
            retry_after: Some(Duration::from_secs(7)),
            network: None,
        };
        assert_eq!(
            default_classify(&f),
            ErrorClass::Retry {
                after: Some(Duration::from_secs(7))
            }
        );
    }

    #[test]
    fn motivo_de_espera_ida_e_volta() {
        for r in [
            WaitReason::Countdown,
            WaitReason::HostLimit,
            WaitReason::Quota,
            WaitReason::Backoff,
            WaitReason::Schedule,
        ] {
            assert_eq!(WaitReason::parse(r.as_str()), Some(r));
        }
    }
}
