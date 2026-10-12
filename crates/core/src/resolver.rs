//! Tudo que o motor enxerga dos plugins: resolver um link e classificar falhas.

use crate::error::{ErrorClass, HostError, HttpFailure, default_classify};
use crate::resolved::{ResolveRequest, Resolved};
use async_trait::async_trait;

/// Como o agendador trata um servidor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HostPolicy {
    /// Downloads ao mesmo tempo no servidor, contando os que esperam o
    /// captcha (o grátis do Rapidgator só aceita um por vez). `None` = só os
    /// limites gerais.
    pub max_active: Option<usize>,
    /// As esperas gravadas do servidor ("próximo download em N min") não
    /// valem: só quando o plugin vai pela conta premium, que não tem essa
    /// espera. Nunca apaga a espera gravada.
    pub ignore_host_waits: bool,
}

/// Transforma um link original em link direto. Implementado pelo registro de
/// plugins (`swoop-hosts`); o motor não conhece servidores específicos.
#[async_trait]
pub trait Resolver: Send + Sync {
    /// Resolve (ou re-resolve, quando `req.attempt > 0`) o link.
    async fn resolve(&self, req: &ResolveRequest) -> Result<Resolved, HostError>;

    /// Regras de agendamento do servidor `host_key`.
    fn host_policy(&self, _host_key: &str) -> HostPolicy {
        HostPolicy::default()
    }

    /// Diz o que fazer com uma falha HTTP durante o download.
    fn classify(&self, _host_key: &str, failure: &HttpFailure) -> ErrorClass {
        default_classify(failure)
    }
}
