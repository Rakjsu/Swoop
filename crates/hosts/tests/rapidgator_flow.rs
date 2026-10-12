//! Fluxo grátis do Rapidgator contra o Rapidgator falso do testsrv (sem
//! rede): contador inteiro antes do link, captcha (Turnstile) pedido ao
//! usuário, resposta só dentro do prazo, link de sessão, esperas e limites
//! do site viram `HostError` claros.

use std::time::{Duration, Instant, SystemTime};
use swoop_core::{
    CaptchaAnswer, CaptchaChallenge, CaptchaKind, HostError, RangeStyle, ResolveRequest, Resolver,
    WaitReason,
};
use swoop_hosts::{Registry, Rules};
use swoop_testsrv::TestServer;
use url::Url;

const ID: &str = "0123456789abcdef0123456789abcdef";

/// Registro que trata o Rapidgator falso como o site (`extra` = mais regras).
fn registry(srv: &TestServer, extra: &str) -> Registry {
    let rules = Rules::with_override(&format!(
        "[rapidgator]\nhosts = [\"127.0.0.1\"]\norigin = \"{}\"\n{extra}",
        srv.rapidgator_origin()
    ))
    .unwrap();
    Registry::new(rules).unwrap()
}

fn req(url: &str) -> ResolveRequest {
    ResolveRequest::new(Url::parse(url).unwrap())
}

/// Resolve até o captcha.
async fn challenge(reg: &Registry, link: &str) -> CaptchaChallenge {
    match reg.resolve(&req(link)).await {
        Err(HostError::Captcha(c)) => *c,
        other => panic!("esperava captcha: {other:?}"),
    }
}

/// Resolve de novo com a resposta do usuário.
async fn answer(
    reg: &Registry,
    link: &str,
    c: CaptchaChallenge,
    token: &str,
) -> Result<swoop_core::Resolved, HostError> {
    let mut again = req(link);
    again.captcha = Some(CaptchaAnswer {
        challenge: c,
        token: token.into(),
    });
    reg.resolve(&again).await
}

fn wait_secs(r: Result<impl std::fmt::Debug, HostError>) -> (u64, WaitReason) {
    match r {
        Err(HostError::Wait { until, reason }) => (
            until.duration_since(SystemTime::now()).unwrap().as_secs(),
            reason,
        ),
        other => panic!("esperava espera: {other:?}"),
    }
}

#[tokio::test]
async fn contador_inteiro_captcha_e_link_de_sessao() {
    let srv = TestServer::start().await.unwrap();
    let reg = registry(&srv, "");
    let link = srv.rapidgator_url(ID, "Relatorio.rar", "secs=2&size=4096");
    let t0 = Instant::now();
    let c = challenge(&reg, &link).await;
    // contador do site (2 s) + margem (1 s) antes de pedir o link
    assert!(
        t0.elapsed() >= Duration::from_secs(3),
        "pediu o link antes do contador"
    );
    assert_eq!(
        c.kind,
        CaptchaKind::Turnstile {
            site_key: "0x4AAAAAAAFAKEsitekeyFAKE".into()
        }
    );
    assert_eq!(c.answer_field, "DownloadCaptchaForm[verifyCode]");
    assert!(c.fields.iter().any(|(k, _)| k == "YII_CSRF_TOKEN"));
    let left = c.expires_at_ms.unwrap() - c.not_before_ms;
    assert_eq!(left, 100_000, "prazo das regras");
    assert_eq!(c.page_url.path(), format!("/file/{ID}/Relatorio.rar.html"));

    let r = answer(&reg, &link, c, "ok")
        .await
        .expect("link depois do captcha");
    assert!(r.url.path().starts_with("/dl/"));
    assert_eq!(
        (r.max_connections, r.resumable, r.range),
        (1, false, RangeStyle::None)
    );
    assert_eq!(r.host_key, "rapidgator");
    let header = |name: &str| {
        r.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    };
    assert!(header("Cookie").is_some_and(|c| c.contains("PHPSESSID=")));
    assert_eq!(
        header("User-Agent").as_deref(),
        Some(Rules::embedded().user_agent.as_str())
    );
    assert!(header("Referer").is_some_and(|r| r.ends_with("/download/captcha")));
    let s = srv.rapidgator_stats();
    assert_eq!(
        (s.timers, s.early_link_calls, s.submits, s.links),
        (1, 0, 1, 1)
    );
}

#[tokio::test]
async fn chave_no_script_e_resposta_errada_pede_outro() {
    let srv = TestServer::start().await.unwrap();
    let reg = registry(&srv, "");
    let link = srv.rapidgator_url(ID, "a.zip", "secs=0&captcha=script");
    let c = challenge(&reg, &link).await;
    assert!(
        matches!(&c.kind, CaptchaKind::Turnstile { site_key } if site_key.starts_with("0x4AAAA"))
    );
    assert!(matches!(
        answer(&reg, &link, c, "errado").await,
        Err(HostError::Captcha(_))
    ));
    let s = srv.rapidgator_stats();
    assert_eq!(
        (s.wrong_answers, s.timers),
        (1, 2),
        "recomeça com outro contador"
    );
}

#[tokio::test]
async fn resposta_fora_do_prazo_nunca_sai() {
    let srv = TestServer::start().await.unwrap();
    // prazo de 1 s: a resposta chega depois e não é enviada
    let reg = registry(
        &srv,
        "captcha_deadline_secs = 1\ncaptcha_min_left_secs = 0\n",
    );
    let link = srv.rapidgator_url(ID, "a.zip", "secs=0");
    let c = challenge(&reg, &link).await;
    tokio::time::sleep(Duration::from_millis(1_100)).await;
    assert!(matches!(
        answer(&reg, &link, c, "ok").await,
        Err(HostError::Captcha(_))
    ));
    let s = srv.rapidgator_stats();
    assert_eq!((s.submits, s.late_submits), (0, 0));
}

#[tokio::test]
async fn site_diz_que_chegou_tarde_vira_espera_do_servidor() {
    let srv = TestServer::start().await.unwrap();
    let reg = registry(&srv, "");
    let link = srv.rapidgator_url(ID, "a.zip", "secs=0&deadline_ms=300");
    let c = challenge(&reg, &link).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let (secs, reason) = wait_secs(answer(&reg, &link, c, "ok").await);
    assert_eq!(reason, WaitReason::HostLimit);
    assert!((59 * 60..=60 * 60).contains(&secs), "{secs}");
    assert_eq!(srv.rapidgator_stats().late_submits, 1);
}

#[tokio::test]
async fn esperas_e_limites_do_site() {
    let srv = TestServer::start().await.unwrap();
    let reg = registry(&srv, "");
    let res = |q: &str| {
        let link = srv.rapidgator_url(ID, "a.zip", q);
        let reg = &reg;
        async move { reg.resolve(&req(&link)).await }
    };
    // um download entregue; o próximo pede "Delay between downloads"
    assert!(res("secs=0&captcha=none&delay=120").await.is_ok());
    let (secs, reason) = wait_secs(res("secs=0&delay=120").await);
    assert_eq!(reason, WaitReason::HostLimit);
    assert!((119 * 60..=120 * 60 + 5).contains(&secs), "{secs}");
    assert_eq!(wait_secs(res("hourly=1").await).1, WaitReason::HostLimit);
    assert_eq!(
        wait_secs(res("one_at_a_time=1").await).1,
        WaitReason::HostLimit
    );
    let big = res(&format!("size={}&size_limit_mb=1", 2 << 20)).await;
    assert!(
        matches!(&big, Err(HostError::PremiumOnly(Some(m))) if m.contains("1 MB")),
        "{big:?}"
    );
    assert!(matches!(
        res("premium_only=1").await,
        Err(HostError::PremiumOnly(_))
    ));
    assert_eq!(res("offline=1").await.unwrap_err(), HostError::Offline);
    assert!(
        matches!(res("cloudflare=1").await, Err(HostError::BrowserRequired(m)) if m.contains("navegador"))
    );
    // contador fora do esperado: espera e recomeça, sem repetir o pedido
    assert_eq!(
        wait_secs(res("secs=0&timer_error=1").await).1,
        WaitReason::Backoff
    );
    assert_eq!(srv.rapidgator_stats().early_link_calls, 0);
}

#[tokio::test]
async fn pagina_nao_entendida_fica_salva_para_diagnostico() {
    let srv = TestServer::start().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let reg = registry(&srv, "").diagnostics_to(dir.path().join("diagnostico"));
    let link = srv.rapidgator_url(ID, "a.zip", "broken=1");
    let err = reg.resolve(&req(&link)).await.unwrap_err();
    let HostError::Changed(msg) = err else {
        panic!("{err:?}")
    };
    assert!(msg.contains("página salva em"), "{msg}");
    let saved: Vec<_> = std::fs::read_dir(dir.path().join("diagnostico"))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(saved.len(), 1);
    assert!(
        saved[0]
            .file_name()
            .to_string_lossy()
            .starts_with("rapidgator-pagina-")
    );
}

#[tokio::test]
async fn coletor_le_nome_e_tamanho() {
    let srv = TestServer::start().await.unwrap();
    let reg = registry(&srv, "");
    let url = |q: &str| Url::parse(&srv.rapidgator_url(ID, "Video.mkv", q)).unwrap();
    let info = reg.check(&url("size=3145728&hourly=1")).await.unwrap();
    assert_eq!(info.name.as_deref(), Some("Video.mkv"));
    assert_eq!(info.size, Some(3 << 20));
    assert_eq!(
        reg.check(&url("offline=1")).await.unwrap_err(),
        HostError::Offline
    );
    assert_eq!(
        srv.rapidgator_stats().timers,
        0,
        "conferir não começa download"
    );
}
