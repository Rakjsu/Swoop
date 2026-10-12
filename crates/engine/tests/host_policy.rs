//! Regras por servidor no agendador (`HostPolicy`): o grátis do Rapidgator
//! só baixa um por vez, contando o que espera o captcha ainda no prazo; outro
//! servidor anda ao mesmo tempo; captcha vencido solta a vaga e só recomeça
//! quando o usuário pede (Recomeçar = `retry`).

mod common;

use async_trait::async_trait;
use common::*;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use swoop_core::{
    CaptchaChallenge, CaptchaKind, DownloadState, HostError, HostPolicy, ResolveRequest, Resolved,
    Resolver,
};
use swoop_testsrv::TestServer;
use url::Url;

/// Prazo do captcha pedido por `um.test/captcha`.
const DEADLINE_MS: i64 = 2_500;

/// `um.test` aceita um download por vez; `/captcha` pede captcha com prazo.
struct OnePerHost {
    file: String,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[async_trait]
impl Resolver for OnePerHost {
    async fn resolve(&self, req: &ResolveRequest) -> Result<Resolved, HostError> {
        if req.url.path() == "/captcha" {
            return Err(HostError::Captcha(Box::new(CaptchaChallenge {
                kind: CaptchaKind::Turnstile {
                    site_key: "chave".into(),
                },
                page_url: req.url.clone(),
                action: req.url.clone(),
                fields: vec![],
                answer_field: "cf-turnstile-response".into(),
                not_before_ms: 0,
                expires_at_ms: Some(now_ms() + DEADLINE_MS),
            })));
        }
        let name = format!("{}{}", req.url.host_str().unwrap(), req.url.path()).replace('/', "-");
        Ok(Resolved::direct(
            Url::parse(&self.file.replace("NOME", &name)).unwrap(),
        ))
    }

    fn host_policy(&self, host_key: &str) -> HostPolicy {
        HostPolicy {
            max_active: (host_key == "um.test").then_some(1),
            ignore_host_waits: false,
        }
    }
}

async fn harness(srv: &TestServer) -> Harness {
    let file = srv.file_url("NOME", &format!("size={}&seed=2&rate={}", 2 * MIB, 2 * MIB));
    let mut s = settings(2);
    s.max_active_downloads = 4;
    Harness::with_resolver(s, Arc::new(OnePerHost { file })).await
}

/// Ativo = resolvendo, baixando ou conferindo.
fn busy(state: DownloadState) -> bool {
    matches!(
        state,
        DownloadState::Resolving | DownloadState::Downloading | DownloadState::Verifying
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn um_por_vez_no_servidor_limitado_e_outro_anda_junto() {
    let srv = TestServer::start().await.unwrap();
    let h = harness(&srv).await;
    let mut limited = Vec::new();
    for n in 1..=3 {
        limited.push(h.add(format!("https://um.test/arquivo{n}")).await);
    }
    let other = h.add("https://outro.test/arquivo".into()).await;

    let (mut peak, mut together) = (0, false);
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(30) {
        let mut on_limited = 0;
        for id in &limited {
            on_limited += usize::from(busy(h.row(*id).await.state));
        }
        let other_busy = busy(h.row(other).await.state);
        peak = peak.max(on_limited);
        together |= on_limited == 1 && other_busy;
        let mut all = limited.clone();
        all.push(other);
        let mut done = true;
        for id in &all {
            done &= h.row(*id).await.state == DownloadState::Completed;
        }
        if done {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    assert_eq!(peak, 1, "mais de um download ao mesmo tempo no um.test");
    assert!(together, "o outro servidor esperou à toa");
    let mut all = limited;
    all.push(other);
    for row in h.wait_final(&all, Duration::from_secs(5)).await {
        completed(&row);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn captcha_no_prazo_segura_a_vaga_e_vencido_espera_o_usuario() {
    let srv = TestServer::start().await.unwrap();
    let h = harness(&srv).await;
    let asked = h.add("https://um.test/captcha".into()).await;
    let t0 = Instant::now();
    while h.row(asked).await.state != DownloadState::CaptchaNeeded {
        assert!(t0.elapsed() < Duration::from_secs(5), "não pediu captcha");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let next = h.add("https://um.test/depois".into()).await;

    // dentro do prazo: a vaga do servidor está com o captcha
    tokio::time::sleep(Duration::from_millis(1_200)).await;
    assert_eq!(h.row(next).await.state, DownloadState::Queued);

    // vencido: a vaga solta e o próximo baixa; o vencido não recomeça sozinho
    completed(&h.wait_final(&[next], Duration::from_secs(20)).await[0]);
    assert_eq!(h.row(asked).await.state, DownloadState::CaptchaNeeded);

    // Recomeçar: volta para a fila e o site pede outro captcha
    h.engine.retry(asked).await.unwrap();
    let t1 = Instant::now();
    loop {
        let row = h.row(asked).await;
        let fresh = row
            .captcha
            .as_deref()
            .and_then(|j| serde_json::from_str::<CaptchaChallenge>(j).ok())
            .is_some_and(|c| !c.expired(now_ms()));
        if row.state == DownloadState::CaptchaNeeded && fresh {
            break;
        }
        assert!(t1.elapsed() < Duration::from_secs(5), "não recomeçou");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
