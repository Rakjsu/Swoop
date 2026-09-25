//! Portão 3d: o link direto expira no meio do download → o motor resolve de
//! novo exatamente uma vez e continua do ponto em que estava.

mod common;

use async_trait::async_trait;
use common::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use swoop_core::{HostError, ResolveRequest, Resolved, Resolver};
use swoop_testsrv::{TestServer, effective_seed, sha256_hex};
use url::Url;

/// Plugin de teste: o 1º link vale meio segundo; os seguintes, dez minutos.
struct Expiring {
    base: String,
    calls: AtomicU32,
}

#[async_trait]
impl Resolver for Expiring {
    async fn resolve(&self, _req: &ResolveRequest) -> Result<Resolved, HostError> {
        let first = self.calls.fetch_add(1, Ordering::SeqCst) == 0;
        let life = if first { 500 } else { 600_000 };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let url = Url::parse(&format!("{}&valid_until={}", self.base, now + life)).unwrap();
        Ok(Resolved::direct(url))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn link_expirado_resolve_uma_vez_e_continua() {
    let srv = TestServer::start().await.unwrap();
    let size = 8 * MIB;
    // A conexão cai a cada 3 MiB (~1,5 s): a reconexão já pega o link vencido.
    let query = format!(
        "size={size}&seed=21&rate={}&drop_after={}",
        2 * MIB,
        3 * MIB
    );
    let resolver = Arc::new(Expiring {
        base: srv.file_url("expira.bin", &query),
        calls: AtomicU32::new(0),
    });
    let h = Harness::with_resolver(settings(1), resolver.clone()).await;
    let id = h
        .add("https://servidor.exemplo/arquivo/expira".into())
        .await;

    let row = &h.wait_final(&[id], Duration::from_secs(60)).await[0];
    assert_eq!(
        sha256_file(&completed(row)),
        sha256_hex(effective_seed(21, 0), size)
    );
    assert_eq!(
        resolver.calls.load(Ordering::SeqCst),
        2,
        "re-resolveu mais de uma vez"
    );
    // Continuou do ponto: além do arquivo, só o que estava em trânsito em
    // cada queda (um pedaço do servidor por queda). Recomeçar do zero
    // passaria de 3 MiB a mais.
    let sent = srv.stats().bytes_sent;
    assert!(
        sent < size + MIB,
        "baixou de novo o que já tinha ({sent} bytes)"
    );
}
