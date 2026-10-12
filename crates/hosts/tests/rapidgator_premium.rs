//! Conta premium do Rapidgator contra a API v2 falsa: login só por POST,
//! sessão reaproveitada (e um login novo quando vence), sem contador nem
//! captcha, várias conexões, MD5 do arquivo; conta grátis vai pelo grátis;
//! arquivo pago não é baixado pela conta; tráfego esgotado vira espera.

use swoop_core::{Account, AccountKind, HostError, ResolveRequest, Resolver, Secret, WaitReason};
use swoop_hosts::{Registry, Rules};
use swoop_testsrv::TestServer;
use swoop_testsrv::rapidgator_api::{FREE_USER, PASSWORD, PREMIUM_USER};
use url::Url;

const ID: &str = "0123456789abcdef0123456789abcdef";

fn registry(srv: &TestServer) -> Registry {
    let rules = Rules::with_override(&format!(
        "[rapidgator]\nhosts = [\"127.0.0.1\"]\norigin = \"{}\"\n",
        srv.rapidgator_origin()
    ))
    .unwrap();
    Registry::new(rules).unwrap()
}

/// Chave da conta: o host da origem com a porta.
fn key(srv: &TestServer) -> String {
    srv.rapidgator_origin()
        .trim_start_matches("http://")
        .to_owned()
}

fn with_account(srv: &TestServer, user: &str, password: &str) -> Registry {
    let reg = registry(srv);
    reg.accounts().set(
        &key(srv),
        Account {
            username: user.into(),
            kind: AccountKind::Login,
            secret: Secret::new(password),
        },
    );
    reg
}

fn req(srv: &TestServer, id: &str) -> ResolveRequest {
    ResolveRequest::new(Url::parse(&srv.rapidgator_url(id, "premium.bin", "secs=0")).unwrap())
}

#[tokio::test]
async fn premium_sem_contador_com_md5_e_sessao_reaproveitada() {
    let srv = TestServer::start().await.unwrap();
    srv.rapidgator().set_premium_file(3 << 20, 9, None);
    let reg = with_account(&srv, PREMIUM_USER, PASSWORD);
    assert!(reg.accepts_account(&key(&srv)));
    assert_eq!(reg.account_kinds(&key(&srv)), vec![AccountKind::Login]);
    // conta ainda sem login: um download entra para descobrir
    let p = reg.host_policy("rapidgator");
    assert_eq!((p.max_active, p.ignore_host_waits), (Some(1), true));

    let r = reg.resolve(&req(&srv, ID)).await.expect("link premium");
    assert!(r.url.path().starts_with("/premium/"));
    assert_eq!((r.max_connections, r.resumable), (5, true));
    assert_eq!(r.size, Some(3 << 20));
    assert_eq!(r.file_name.as_deref(), Some("premium.bin"));
    assert!(matches!(r.integrity, Some(swoop_core::Integrity::Md5(ref h)) if h.len() == 32));
    assert_eq!(r.host_key, "rapidgator");
    let p = reg.host_policy("rapidgator");
    assert_eq!((p.max_active, p.ignore_host_waits), (None, true));

    reg.resolve(&req(&srv, ID)).await.unwrap();
    let s = srv.rapidgator_stats();
    assert_eq!(
        (s.logins, s.premium_links, s.timers),
        (1, 2, 0),
        "um login para dois links, sem contador"
    );
    assert_eq!(s.password_in_query, 0, "senha na URL");

    // sessão vencida no site: exatamente um login novo
    srv.rapidgator().expire_sessions();
    reg.resolve(&req(&srv, ID)).await.unwrap();
    assert_eq!(srv.rapidgator_stats().logins, 2);
}

#[tokio::test]
async fn conta_conferida_e_recusas() {
    let srv = TestServer::start().await.unwrap();
    let reg = with_account(&srv, PREMIUM_USER, PASSWORD);
    let acc = reg.accounts().get(&key(&srv)).unwrap();
    let info = reg.account_info(&key(&srv), &acc).await.unwrap();
    assert!(info.premium);
    assert_eq!(info.premium_until_ms, Some(4_102_444_800_000));
    assert_eq!(info.traffic_left, Some(4_105_663_543_401));

    let bad = with_account(&srv, PREMIUM_USER, "errada");
    let err = bad.resolve(&req(&srv, ID)).await.unwrap_err();
    assert!(
        matches!(&err, HostError::Account(m) if m.contains("senha")),
        "{err:?}"
    );

    let paid = reg
        .resolve(&req(&srv, "ffff456789abcdef0123456789abcdef"))
        .await
        .unwrap_err();
    assert!(
        matches!(&paid, HostError::Account(m) if m.contains("pago")),
        "{paid:?}"
    );

    srv.rapidgator().set_traffic_exceeded(true);
    let quota = reg.resolve(&req(&srv, ID)).await.unwrap_err();
    assert!(
        matches!(
            quota,
            HostError::Wait {
                reason: WaitReason::Quota,
                ..
            }
        ),
        "{quota:?}"
    );
    assert_eq!(srv.rapidgator_stats().premium_links, 0);
}

#[tokio::test]
async fn conta_gratis_segue_pelo_gratis() {
    let srv = TestServer::start().await.unwrap();
    let reg = with_account(&srv, FREE_USER, PASSWORD);
    let res = reg.resolve(&req(&srv, ID)).await;
    assert!(matches!(res, Err(HostError::Captcha(_))), "{res:?}");
    let p = reg.host_policy("rapidgator");
    assert_eq!((p.max_active, p.ignore_host_waits), (Some(1), false));
    let s = srv.rapidgator_stats();
    assert_eq!((s.logins, s.timers, s.premium_links), (1, 1, 0));
}
