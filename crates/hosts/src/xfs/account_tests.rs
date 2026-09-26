use super::*;
use crate::Rules;

fn rules() -> XfsRules {
    Rules::embedded().xfs
}

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/xfs/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).unwrap()
}

fn page() -> Url {
    Url::parse("https://fastfile.cc/abcdefgh1234").unwrap()
}

/// 2026-09-26 00:00:00 UTC.
const NOW: i64 = 1_790_380_800_000;

#[test]
fn datas_do_site_em_ms() {
    assert_eq!(datetime_ms("1970-01-01 00:00:00"), Some(0));
    assert_eq!(datetime_ms("2026-09-26"), Some(NOW));
    assert_eq!(datetime_ms("2000-03-01 01:02:03"), Some(951_872_523_000));
    assert_eq!(datetime_ms("2026-13-01"), None);
    assert_eq!(datetime_ms("amanhã"), None);
}

#[test]
fn api_da_chave_link_conta_e_erros() {
    let link = api_link(
        r#"{"msg":"OK","status":200,"result":{"url":"https://s3.fastfile.cc/d/xyz/Relatorio.pdf","size":"13107200"}}"#,
        "fastfile.cc",
        &page(),
    )
    .unwrap();
    assert_eq!(link.host_str(), Some("s3.fastfile.cc"));

    let info = api_info(
        r#"{"msg":"OK","status":"200","result":{"login":"ana","premium_expire":"2027-01-01 00:00:00","traffic_left":"102400"}}"#,
        "fastfile.cc",
        NOW,
    )
    .unwrap();
    assert!(info.premium);
    assert_eq!(info.premium_until_ms, datetime_ms("2027-01-01"));
    assert_eq!(info.traffic_left, Some(102_400 * MIB));

    // premium vencido vale mais que a bandeira
    let old = api_info(
        r#"{"status":200,"result":{"premium":1,"premium_expire":"2020-01-01 00:00:00"}}"#,
        "fastfile.cc",
        NOW,
    )
    .unwrap();
    assert!(!old.premium);

    let bad = api_info(r#"{"msg":"Invalid key","status":403}"#, "fastfile.cc", NOW);
    assert_eq!(
        bad,
        Err(HostError::Account(
            "fastfile.cc recusou a conta: Invalid key".into()
        ))
    );
    assert_eq!(
        api_link(r#"{"msg":"no file","status":404}"#, "fastfile.cc", &page()),
        Err(HostError::Offline)
    );
    assert!(matches!(
        api_link("<html>erro</html>", "fastfile.cc", &page()),
        Err(HostError::Changed(_))
    ));
}

#[test]
fn login_aceito_ou_recusado() {
    assert_eq!(
        login_result(&fixture("fastfile_login_ok.html"), &rules(), "fastfile.cc"),
        Ok(())
    );
    assert!(matches!(
        login_result(&fixture("fastfile_login_bad.html"), &rules(), "fastfile.cc"),
        Err(HostError::Account(m)) if m.contains("usuário ou a senha")
    ));
    assert!(matches!(
        login_result("<html>?</html>", &rules(), "fastfile.cc"),
        Err(HostError::Changed(_))
    ));
}

#[test]
fn pagina_da_conta_com_e_sem_premium() {
    let info = page_info(&fixture("fastfile_my_account.html"), &rules(), NOW);
    assert!(info.premium);
    assert_eq!(info.premium_until_ms, datetime_ms("2027-03-15 10:20:30"));
    let free = page_info(&fixture("fastfile_my_account_free.html"), &rules(), NOW);
    assert_eq!(free, AccountInfo::default());
}

#[test]
fn pagina_do_arquivo_com_sessao_premium() {
    let now = SystemTime::UNIX_EPOCH;
    let host = "fastfile.cc";
    let Premium::Form(form) = premium_page(
        &fixture("fastfile_premium_page.html"),
        &page(),
        &rules(),
        host,
        now,
    )
    .unwrap() else {
        panic!("esperava o formulário premium");
    };
    assert!(form.fields.contains(&("op".into(), "download2".into())));
    assert!(form.fields.contains(&("method_premium".into(), "1".into())));

    // escolha grátis/premium: vai o botão premium
    let Premium::Form(form) = premium_page(
        &fixture("fastfile_page1.html"),
        &page(),
        &rules(),
        host,
        now,
    )
    .unwrap() else {
        panic!("esperava o formulário da escolha");
    };
    assert!(!form.fields.iter().any(|(k, _)| k == "method_free"));
    assert!(form.fields.iter().any(|(k, _)| k == "method_premium"));

    assert!(matches!(
        premium_page(
            &fixture("fastfile_final.html"),
            &page(),
            &rules(),
            host,
            now
        ),
        Ok(Premium::Link(_))
    ));
    // o site ainda trata a conta como grátis
    for f in [
        "fastfile_premium.html",
        "fastfile_wait.html",
        "fastfile_page2.html",
    ] {
        assert!(
            matches!(
                premium_page(&fixture(f), &page(), &rules(), host, now),
                Err(HostError::Account(ref m)) if m.contains("não é premium")
            ),
            "{f}"
        );
    }
    assert_eq!(
        premium_page(
            &fixture("fastfile_notfound.html"),
            &page(),
            &rules(),
            host,
            now
        ),
        Err(HostError::Offline)
    );
}
