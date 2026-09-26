//! Conta premium do XFileSharing contra o XFS falso do testsrv: chave da API
//! e login dão o link sem contador nem captcha, com 5 conexões e retomada;
//! chave ou senha erradas viram erro claro de conta; a conferência lê a
//! validade do premium.

use swoop_core::{Account, AccountKind, HostError, ResolveRequest, Resolver, Secret};
use swoop_hosts::{Registry, Rules};
use swoop_testsrv::TestServer;
use swoop_testsrv::xfs_premium::{FREE_KEY, KEY, PASS, USER};
use url::Url;

const CODE: &str = "abcdefgh1234";

/// Registro que trata o testsrv (http, 127.0.0.1) como site XFileSharing.
fn registry() -> Registry {
    let rules = Rules::with_override("[xfs]\nhosts = [\"127.0.0.1\"]\naccount_scheme = \"http\"\n")
        .unwrap();
    Registry::new(rules).unwrap()
}

fn account(kind: AccountKind, user: &str, secret: &str) -> Account {
    Account {
        username: user.into(),
        kind,
        secret: Secret::new(secret),
    }
}

/// Resolve o link do arquivo com a conta dada cadastrada no servidor.
async fn resolve_with(srv: &TestServer, acc: Account) -> Result<swoop_core::Resolved, HostError> {
    let reg = registry();
    let key = format!("127.0.0.1:{}", srv.addr().port());
    reg.accounts().set(&key, acc);
    let link = Url::parse(&srv.xfs_url(CODE, "countdown=30")).unwrap();
    reg.resolve(&ResolveRequest::new(link)).await
}

#[tokio::test]
async fn chave_da_api_da_o_link_premium() {
    let srv = TestServer::start().await.unwrap();
    let r = resolve_with(&srv, account(AccountKind::ApiKey, "", KEY))
        .await
        .expect("link premium");
    assert!(r.url.path().starts_with("/file/abcdefgh1234.bin"));
    assert_eq!(r.max_connections, 5);
    assert!(r.resumable);
    let stats = srv.xfs_stats();
    assert_eq!(
        (stats.premium_links, stats.links),
        (1, 0),
        "sem o fluxo grátis"
    );

    let err = resolve_with(&srv, account(AccountKind::ApiKey, "", "errada")).await;
    assert!(matches!(err, Err(HostError::Account(m)) if m.contains("Invalid key")));
}

#[tokio::test]
async fn login_da_o_link_premium_pelo_formulario() {
    let srv = TestServer::start().await.unwrap();
    // contador de 30 s no grátis: o premium não pode esperar por ele
    let t0 = std::time::Instant::now();
    let r = resolve_with(&srv, account(AccountKind::Login, USER, PASS))
        .await
        .expect("link premium");
    assert!(t0.elapsed() < std::time::Duration::from_secs(5));
    assert!(r.url.path().starts_with("/file/abcdefgh1234.bin"));
    assert_eq!(r.max_connections, 5);
    assert_eq!(srv.xfs_stats().premium_links, 1);

    let err = resolve_with(&srv, account(AccountKind::Login, USER, "errada")).await;
    assert!(matches!(err, Err(HostError::Account(m)) if m.contains("usuário ou a senha")));
}

#[tokio::test]
async fn conferencia_da_conta() {
    let srv = TestServer::start().await.unwrap();
    let reg = registry();
    let key = format!("127.0.0.1:{}", srv.addr().port());
    assert!(reg.accepts_account(&key));
    assert!(!reg.accepts_account("outro.example"));
    assert_eq!(reg.account_hosts(), vec!["127.0.0.1".to_owned()]);

    let premium = reg
        .account_info(&key, &account(AccountKind::ApiKey, "", KEY))
        .await
        .unwrap();
    assert!(premium.premium);
    assert_eq!(premium.traffic_left, Some(102_400 * 1024 * 1024));
    let free = reg
        .account_info(&key, &account(AccountKind::ApiKey, "", FREE_KEY))
        .await
        .unwrap();
    assert!(!free.premium && free.premium_until_ms.is_some());
    let login = reg
        .account_info(&key, &account(AccountKind::Login, USER, PASS))
        .await
        .unwrap();
    assert!(login.premium);
    assert!(matches!(
        reg.account_info(&key, &account(AccountKind::Login, USER, "x"))
            .await,
        Err(HostError::Account(_))
    ));
}
