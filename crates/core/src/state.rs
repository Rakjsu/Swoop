//! Máquina de estados de um download.
//!
//! Toda mudança de estado passa por `next`, que é pura: o motor decide o
//! evento, `next` diz se a transição é válida. Detalhes (até quando esperar,
//! qual erro) ficam em colunas próprias do banco, não no estado.
//!
//! ```text
//! Queued → Resolving → Downloading → Verifying → Downloaded → Completed
//! Resolving|Downloading → Waiting → Queued          (espera/cota/backoff)
//! Downloading → Resolving                            (link expirou)
//! Resolving|Downloading|Verifying → Failed → Queued  (nova tentativa)
//! qualquer não final → Paused → Queued
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;

/// Estado persistido de um download.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    Queued,
    Resolving,
    Downloading,
    Verifying,
    Downloaded,
    Completed,
    Waiting,
    Paused,
    Failed,
    /// O servidor pediu captcha: espera o usuário (sem ocupar vaga).
    CaptchaNeeded,
}

/// O que aconteceu com o download (entrada da máquina de estados).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Event {
    /// O agendador escolheu o download.
    Start,
    /// O resolvedor devolveu o link direto.
    Resolved,
    /// Todos os bytes chegaram ao disco.
    Transferred,
    /// Integridade conferida (ou não havia o que conferir).
    Verified,
    /// Hash diferente do anunciado.
    IntegrityFailed,
    /// Pós-processo terminou (extração entra na fase 5).
    Finalized,
    /// Servidor pediu espera (contador, cota, limite, backoff).
    Wait,
    /// A espera acabou.
    WaitElapsed,
    /// O link expirou no meio: resolver de novo.
    Reresolve,
    /// Usuário pausou.
    Pause,
    /// Usuário retomou.
    Resume,
    /// Erro sem recuperação automática.
    Fail,
    /// Usuário pediu nova tentativa.
    Retry,
    /// O servidor pediu captcha.
    NeedCaptcha,
    /// O usuário resolveu o captcha: volta para a fila.
    CaptchaSolved,
}

/// Transição recusada: o motor tentou algo fora da tabela.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("transição inválida: {from} + {event:?}")]
pub struct InvalidTransition {
    pub from: DownloadState,
    pub event: Event,
}

impl DownloadState {
    pub const ALL: [DownloadState; 10] = [
        Self::Queued,
        Self::Resolving,
        Self::Downloading,
        Self::Verifying,
        Self::Downloaded,
        Self::Completed,
        Self::Waiting,
        Self::Paused,
        Self::Failed,
        Self::CaptchaNeeded,
    ];

    /// Nome estável usado no banco.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Resolving => "resolving",
            Self::Downloading => "downloading",
            Self::Verifying => "verifying",
            Self::Downloaded => "downloaded",
            Self::Completed => "completed",
            Self::Waiting => "waiting",
            Self::Paused => "paused",
            Self::Failed => "failed",
            Self::CaptchaNeeded => "captcha_needed",
        }
    }

    /// Lê o nome gravado no banco.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|st| st.as_str() == s)
    }

    /// Estado em que o motor está trabalhando ativamente no download.
    pub fn is_active(self) -> bool {
        matches!(self, Self::Resolving | Self::Downloading | Self::Verifying)
    }

    /// Não há mais nada a fazer (sem ação do usuário).
    pub fn is_final(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }

    /// Estado ao reabrir o app: o que estava em andamento volta para a fila.
    /// Captcha pendente também, porque a sessão do site vivia na memória.
    pub fn recover(self) -> Self {
        if self.is_active() || self == Self::CaptchaNeeded {
            Self::Queued
        } else {
            self
        }
    }

    /// Aplica um evento; erro se a transição não existe.
    pub fn next(self, event: Event) -> Result<Self, InvalidTransition> {
        use DownloadState as S;
        use Event as E;
        let to = match (self, event) {
            (S::Queued, E::Start) => S::Resolving,
            (S::Resolving, E::Resolved) => S::Downloading,
            (S::Downloading, E::Transferred) => S::Verifying,
            (S::Verifying, E::Verified) => S::Downloaded,
            (S::Verifying, E::IntegrityFailed) => S::Failed,
            (S::Downloaded, E::Finalized) => S::Completed,
            (S::Resolving | S::Downloading, E::Wait) => S::Waiting,
            (S::Waiting, E::WaitElapsed) => S::Queued,
            (S::Downloading, E::Reresolve) => S::Resolving,
            (S::Resolving | S::Downloading | S::Verifying, E::Fail) => S::Failed,
            (S::Failed, E::Retry) => S::Queued,
            (S::Queued | S::Resolving | S::Downloading | S::Verifying | S::Waiting, E::Pause) => {
                S::Paused
            }
            (S::Paused, E::Resume) => S::Queued,
            (S::Resolving, E::NeedCaptcha) => S::CaptchaNeeded,
            (S::CaptchaNeeded, E::CaptchaSolved) => S::Queued,
            (S::CaptchaNeeded, E::Pause) => S::Paused,
            (S::CaptchaNeeded, E::Fail) => S::Failed,
            (from, event) => return Err(InvalidTransition { from, event }),
        };
        Ok(to)
    }
}

impl fmt::Display for DownloadState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DownloadState as S;
    use Event as E;

    const EVENTS: [Event; 15] = [
        E::Start,
        E::Resolved,
        E::Transferred,
        E::Verified,
        E::IntegrityFailed,
        E::Finalized,
        E::Wait,
        E::WaitElapsed,
        E::Reresolve,
        E::Pause,
        E::Resume,
        E::Fail,
        E::Retry,
        E::NeedCaptcha,
        E::CaptchaSolved,
    ];

    /// Tabela completa das transições válidas; todo o resto é recusado.
    const VALID: &[(DownloadState, Event, DownloadState)] = &[
        (S::Queued, E::Start, S::Resolving),
        (S::Queued, E::Pause, S::Paused),
        (S::Resolving, E::Resolved, S::Downloading),
        (S::Resolving, E::Wait, S::Waiting),
        (S::Resolving, E::Fail, S::Failed),
        (S::Resolving, E::Pause, S::Paused),
        (S::Downloading, E::Transferred, S::Verifying),
        (S::Downloading, E::Wait, S::Waiting),
        (S::Downloading, E::Reresolve, S::Resolving),
        (S::Downloading, E::Fail, S::Failed),
        (S::Downloading, E::Pause, S::Paused),
        (S::Verifying, E::Verified, S::Downloaded),
        (S::Verifying, E::IntegrityFailed, S::Failed),
        (S::Verifying, E::Fail, S::Failed),
        (S::Verifying, E::Pause, S::Paused),
        (S::Downloaded, E::Finalized, S::Completed),
        (S::Waiting, E::WaitElapsed, S::Queued),
        (S::Waiting, E::Pause, S::Paused),
        (S::Paused, E::Resume, S::Queued),
        (S::Failed, E::Retry, S::Queued),
        (S::Resolving, E::NeedCaptcha, S::CaptchaNeeded),
        (S::CaptchaNeeded, E::CaptchaSolved, S::Queued),
        (S::CaptchaNeeded, E::Pause, S::Paused),
        (S::CaptchaNeeded, E::Fail, S::Failed),
    ];

    #[test]
    fn todos_os_pares_seguem_a_tabela() {
        for from in DownloadState::ALL {
            for event in EVENTS {
                let expected = VALID
                    .iter()
                    .find(|(f, e, _)| *f == from && *e == event)
                    .map(|(_, _, to)| *to);
                match expected {
                    Some(to) => assert_eq!(from.next(event), Ok(to), "{from} + {event:?}"),
                    None => assert!(
                        from.next(event).is_err(),
                        "{from} + {event:?} deveria falhar"
                    ),
                }
            }
        }
    }

    #[test]
    fn nome_no_banco_ida_e_volta() {
        for st in DownloadState::ALL {
            assert_eq!(DownloadState::parse(st.as_str()), Some(st));
        }
        assert_eq!(DownloadState::parse("inexistente"), None);
    }

    #[test]
    fn reabrir_devolve_ativos_para_a_fila() {
        assert_eq!(S::Downloading.recover(), S::Queued);
        assert_eq!(S::Resolving.recover(), S::Queued);
        assert_eq!(S::Verifying.recover(), S::Queued);
        assert_eq!(S::Waiting.recover(), S::Waiting);
        assert_eq!(S::CaptchaNeeded.recover(), S::Queued);
        assert_eq!(S::Paused.recover(), S::Paused);
        assert_eq!(S::Completed.recover(), S::Completed);
    }
}
