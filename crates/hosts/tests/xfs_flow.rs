//! Fluxo grátis do XFileSharing contra o XFS falso do testsrv (sem rede):
//! contador nunca pulado, captcha pedido ao usuário, resposta errada pede de
//! novo, só premium e espera entre downloads.

use std::time::{Duration, Instant};
use swoop_core::{CaptchaAnswer, CaptchaKind, HostError, ResolveRequest, Resolver, WaitReason};
use swoop_hosts::{Registry, Rules};
use swoop_testsrv::TestServer;
use url::Url;

const CODE: &str = "abcdefgh1234";

/// Registro que trata o testsrv (127.0.0.1) como site XFileSharing.
fn registry() -> Registry {
    let rules = Rules::with_override("[xfs]\nhosts = [\"127.0.0.1\"]\n").unwrap();
    Registry::new(rules).unwrap()
}

fn req(url: &str) -> ResolveRequest {
    ResolveRequest::new(Url::parse(url).unwrap())
}

#[tokio::test]
async fn captcha_respeita_o_contador() {
    let srv = TestServer::start().await.unwrap();
    let reg = registry();
    let link = srv.xfs_url(CODE, "countdown=2&captcha=recaptcha&size=4096");
    let t0 = Instant::now();
    let Err(HostError::Captcha(challenge)) = reg.resolve(&req(&link)).await else {
        panic!("esperava captcha");
    };
    assert!(
        matches!(challenge.kind, CaptchaKind::Recaptcha2 { ref site_key } if site_key == "test-site-key")
    );
    assert_eq!(challenge.answer_field, "g-recaptcha-response");

    // O usuário responde na hora: o plugin espera o contador antes de enviar.
    let mut again = req(&link);
    again.captcha = Some(CaptchaAnswer {
        challenge: *challenge,
        token: "ok".into(),
    });
    let resolved = reg.resolve(&again).await.expect("link depois do captcha");
    assert!(
        t0.elapsed() >= Duration::from_secs(2),
        "enviou antes do contador"
    );
    assert!(resolved.url.path().starts_with("/file/abcdefgh1234.bin"));
    assert_eq!(resolved.max_connections, 1);
    assert!(!resolved.resumable);
    assert_eq!(
        resolved.host_key,
        format!("127.0.0.1:{}", srv.addr().port())
    );
    let stats = srv.xfs_stats();
    assert_eq!((stats.early_submits, stats.links), (0, 1));
}

#[tokio::test]
async fn resposta_errada_pede_outro_captcha() {
    let srv = TestServer::start().await.unwrap();
    let reg = registry();
    let link = srv.xfs_url(CODE, "countdown=0&captcha=image");
    let Err(HostError::Captcha(challenge)) = reg.resolve(&req(&link)).await else {
        panic!("esperava captcha");
    };
    assert!(matches!(challenge.kind, CaptchaKind::Image { .. }));
    let mut again = req(&link);
    again.captcha = Some(CaptchaAnswer {
        challenge: *challenge,
        token: "errado".into(),
    });
    assert!(matches!(
        reg.resolve(&again).await,
        Err(HostError::Captcha(_))
    ));
    assert_eq!(srv.xfs_stats().wrong_answers, 1);
}

#[tokio::test]
async fn sem_captcha_premium_e_espera_entre_downloads() {
    let srv = TestServer::start().await.unwrap();
    let reg = registry();
    let free = srv.xfs_url(CODE, "countdown=1&captcha=none&wait_between=60");
    let t0 = Instant::now();
    reg.resolve(&req(&free)).await.expect("link sem captcha");
    assert!(t0.elapsed() >= Duration::from_secs(1));
    // Logo depois, o site manda esperar: vale para o servidor inteiro.
    match reg.resolve(&req(&free)).await {
        Err(HostError::Wait { reason, .. }) => assert_eq!(reason, WaitReason::HostLimit),
        other => panic!("esperava espera entre downloads: {other:?}"),
    }
    let premium = srv.xfs_url(CODE, "premium_only=1");
    assert_eq!(
        reg.resolve(&req(&premium)).await.unwrap_err(),
        HostError::PremiumOnly
    );
}
