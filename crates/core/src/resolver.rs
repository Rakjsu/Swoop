//! Tudo que o motor enxerga dos plugins: resolver um link e classificar falhas.

use crate::error::{ErrorClass, HostError, HttpFailure, default_classify};
use crate::resolved::{ResolveRequest, Resolved};
use async_trait::async_trait;

/// Transforma um link original em link direto. Implementado pelo registro de
/// plugins (`swoop-hosts`); o motor não conhece servidores específicos.
#[async_trait]
pub trait Resolver: Send + Sync {
    /// Resolve (ou re-resolve, quando `req.attempt > 0`) o link.
    async fn resolve(&self, req: &ResolveRequest) -> Result<Resolved, HostError>;

    /// Diz o que fazer com uma falha HTTP durante o download.
    fn classify(&self, _host_key: &str, failure: &HttpFailure) -> ErrorClass {
        default_classify(failure)
    }
}
