//! Captcha pedido por um servidor: o app lê o desafio para montar a janela e
//! devolve a resposta do usuário. O token nunca vai para o log.

use crate::{Service, ServiceError};
use swoop_api::Push;
use swoop_core::{CaptchaChallenge, DownloadId, DownloadState, Event};
use swoop_store::{captcha, downloads, now_ms};

/// Maior token aceito (os do reCAPTCHA/hCaptcha têm poucos KiB).
const MAX_TOKEN: usize = 16 * 1024;
/// Resposta depois do prazo do site.
pub const CAPTCHA_EXPIRED: &str =
    "o prazo do site para este captcha acabou; clique em Recomeçar para pedir outro";

impl Service {
    /// Desafio pendente do download (`None` se não estiver esperando captcha).
    pub async fn captcha_challenge(
        &self,
        id: DownloadId,
    ) -> Result<Option<CaptchaChallenge>, ServiceError> {
        let json = self
            .store
            .call(move |c| {
                let waiting =
                    downloads::get(c, id)?.is_some_and(|r| r.state == DownloadState::CaptchaNeeded);
                if waiting {
                    captcha::challenge(c, id)
                } else {
                    Ok(None)
                }
            })
            .await?;
        Ok(json.and_then(|j| serde_json::from_str(&j).ok()))
    }

    /// Resposta do usuário: guarda o token, devolve o download à fila e
    /// acorda o motor (o plugin envia o formulário depois do contador).
    pub async fn captcha_solved(&self, id: DownloadId, token: String) -> Result<(), ServiceError> {
        let token = token.trim().to_owned();
        if token.is_empty() || token.len() > MAX_TOKEN || token.chars().any(char::is_control) {
            return Err(ServiceError::BadInput(
                "resposta de captcha inválida".into(),
            ));
        }
        let expired = self
            .store
            .call(move |c| {
                let challenge = captcha::challenge(c, id)?
                    .and_then(|j| serde_json::from_str::<CaptchaChallenge>(&j).ok());
                if challenge.is_some_and(|ch| ch.expired(now_ms())) {
                    // Nunca enviar depois do prazo (o site pune): fica para o
                    // usuário recomeçar.
                    return Ok(true);
                }
                downloads::transition(c, id, Event::CaptchaSolved)?;
                captcha::set_token(c, id, &token)?;
                Ok(false)
            })
            .await?;
        if expired {
            return Err(ServiceError::BadInput(CAPTCHA_EXPIRED.into()));
        }
        self.engine.wake();
        let _ = self.pushes.send(Push::Changed);
        Ok(())
    }
}
