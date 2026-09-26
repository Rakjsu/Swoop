//! Captcha pedido por um servidor: o app lê o desafio para montar a janela e
//! devolve a resposta do usuário. O token nunca vai para o log.

use crate::{Service, ServiceError};
use swoop_api::Push;
use swoop_core::{CaptchaChallenge, DownloadId, DownloadState, Event};
use swoop_store::{captcha, downloads};

/// Maior token aceito (os do reCAPTCHA/hCaptcha têm poucos KiB).
const MAX_TOKEN: usize = 16 * 1024;

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
        self.store
            .call(move |c| {
                downloads::transition(c, id, Event::CaptchaSolved)?;
                captcha::set_token(c, id, &token)
            })
            .await?;
        self.engine.wake();
        let _ = self.pushes.send(Push::Changed);
        Ok(())
    }
}
