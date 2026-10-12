use super::*;
use crate::Rules;

fn rules() -> RapidgatorRules {
    Rules::embedded().rapidgator
}

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/rapidgator/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(path).unwrap()
}

fn ok(name: &str) -> Value {
    response(200, &fixture(name), &rules()).unwrap()
}

fn refused(name: &str) -> Refused {
    response(200, &fixture(name), &rules()).unwrap_err()
}

const NOW: i64 = 1_791_000_000_000;

#[test]
fn login_da_o_token_e_a_situacao_da_conta() {
    let resp = ok("api_login_ok.json");
    assert_eq!(token(&resp).as_deref(), Some("fake-token-0001"));
    let i = info(&resp, NOW);
    assert!(i.premium);
    assert_eq!(i.premium_until_ms, Some(4_102_444_800_000));
    assert_eq!(i.traffic_left, Some(4_105_663_543_401));
    let free = info(&ok("api_user_free.json"), NOW);
    assert!(!free.premium);
    assert_eq!((free.premium_until_ms, free.traffic_left), (None, None));
    // premium vencido não vale
    let gone = info(&ok("api_user_premium.json"), 4_102_444_800_001);
    assert!(!gone.premium);
}

#[test]
fn recusas_viram_erros_claros() {
    assert!(
        matches!(refused("api_login_bad.json"), Refused::Error(HostError::Account(m)) if m.contains("senha"))
    );
    assert_eq!(refused("api_401.json"), Refused::Relogin);
    assert!(matches!(
        refused("api_423.json"),
        Refused::Error(HostError::Wait {
            reason: WaitReason::Quota,
            ..
        })
    ));
    let ip = r#"{"response":null,"status":403,"details":"Error: You requested login to your account from unusual Ip address"}"#;
    assert!(
        matches!(response(200, ip, &rules()), Err(Refused::Error(HostError::Account(m))) if m.contains("e-mail"))
    );
    let code = r#"{"response":null,"status":401,"details":"Invalid auth code"}"#;
    assert!(
        matches!(response(200, code, &rules()), Err(Refused::Error(HostError::Account(m))) if m.contains("duas etapas"))
    );
    let wait = r#"{"response":null,"status":429,"details":"Please wait"}"#;
    assert!(matches!(
        response(200, wait, &rules()),
        Err(Refused::Error(HostError::Wait {
            reason: WaitReason::Backoff,
            ..
        }))
    ));
    assert!(matches!(
        response(502, "<html>bad gateway</html>", &rules()),
        Err(Refused::Error(HostError::Changed(_)))
    ));
    let locked = r#"{"response":null,"status":403,"details":"Error: ACCOUNT LOCKED FOR VIOLATION OF OUR TERMS. PLEASE CONTACT SUPPORT."}"#;
    assert!(
        matches!(response(200, locked, &rules()), Err(Refused::Error(HostError::Account(m))) if m.contains("bloqueou"))
    );
    // "blocked" de arquivo não é conta bloqueada
    let file = r#"{"response":null,"status":404,"details":"Error: File is blocked"}"#;
    assert_eq!(
        response(200, file, &rules()),
        Err(Refused::Error(HostError::Offline))
    );
}

#[test]
fn arquivo_pela_api_e_link_premium() {
    let meta = file_meta(&ok("api_file_info.json"), &rules());
    assert_eq!(meta.name.as_deref(), Some("Relatorio Anual 2026.rar"));
    assert_eq!(meta.size, Some(156_762_112));
    assert_eq!(
        meta.md5.as_deref(),
        Some("0f1e2d3c4b5a69788796a5b4c3d2e1f0")
    );
    assert!(!meta.paid);
    let paid = file_meta(&ok("api_file_info_paid.json"), &rules());
    assert!(paid.paid);
    assert_eq!(paid.md5, None, "hash vazio não vira conferência");
    let url = download_url(&ok("api_download_ok.json")).unwrap();
    assert_eq!(url.host_str(), Some("pr42.rapidgator.net"));
    let later = serde_json::json!({"download_url": "https://x/y", "delay": 30});
    assert!(matches!(download_url(&later), Err(HostError::Wait { .. })));
    let js = serde_json::json!({"download_url": "javascript:alert(1)"});
    assert!(matches!(download_url(&js), Err(HostError::Changed(_))));
}
