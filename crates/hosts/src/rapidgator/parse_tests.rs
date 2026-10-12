use super::*;
use crate::Rules;
use std::time::UNIX_EPOCH;

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

fn url(s: &str) -> Url {
    Url::parse(s).unwrap()
}

const T0: SystemTime = UNIX_EPOCH;
const ID: &str = "0123456789abcdef0123456789abcdef";

fn id_of(s: &str) -> Option<String> {
    file_target(&url(s), &rules()).map(|(id, _)| id)
}

#[test]
fn formatos_de_link() {
    let ok = [
        format!("https://rapidgator.net/file/{ID}"),
        format!("https://rapidgator.net/file/{ID}/"),
        format!("https://rapidgator.net/file/{ID}/Relatorio_Anual.rar.html"),
        format!("http://rapidgator.net/file/{ID}/Relatorio_Anual.rar.html"),
        format!("https://www.rapidgator.net/file/{ID}/a.rar.html"),
        format!("https://rg.to/file/{ID}"),
        format!("https://www.rg.to/file/{ID}/a.zip.html"),
        format!("https://rapidgator.asia/file/{ID}/a.zip.html"),
        format!(
            "https://rapidgator.net/file/{}/A.zip.html",
            ID.to_uppercase()
        ),
        format!("https://rapidgator.net/file/{ID}/nome%20com%20espa%C3%A7o.mkv.html"),
        "https://rapidgator.net/file/735569057".to_owned(),
        "https://rapidgator.net/file/735569057/antigo.part1.rar.html".to_owned(),
        format!("https://rapidgator.net/file/{ID}?ref=forum"),
    ];
    for link in &ok {
        let got = id_of(link);
        assert!(got.is_some(), "{link}");
    }
    assert_eq!(id_of(&ok[2]).as_deref(), Some(ID));
    assert_eq!(id_of(&ok[10]).as_deref(), Some("735569057"));

    let not = [
        "https://rapidgator.net/".to_owned(),
        "https://rapidgator.net/file/".to_owned(),
        "https://rapidgator.net/auth/login".to_owned(),
        "https://rapidgator.net/file/xyz".to_owned(),
        format!("https://notrapidgator.net/file/{ID}"),
        format!("https://rapidgator.net.evil.example/file/{ID}"),
        format!("https://rapidgator.net/folder/123/{ID}.html"),
        format!("https://rapidgator.net/file/{ID}/a/b.html"),
    ];
    for link in &not {
        assert_eq!(id_of(link), None, "{link}");
    }
}

#[test]
fn link_normalizado_na_origem_das_regras() {
    let (_, canonical) = file_target(
        &url(&format!("http://www.rg.to/file/{ID}/a.rar.html?x=1")),
        &rules(),
    )
    .unwrap();
    assert_eq!(
        canonical.as_str(),
        format!("https://rapidgator.net/file/{ID}/a.rar.html?x=1")
    );
    let folder = folder_target(&url("https://rg.to/folder/4521/Colecao.html"), &rules()).unwrap();
    assert_eq!(
        folder.as_str(),
        "https://rapidgator.net/folder/4521/Colecao.html"
    );
    assert!(folder_target(&url(&format!("https://rapidgator.net/file/{ID}")), &rules()).is_none());
}

#[test]
fn pagina_do_arquivo_tem_o_download_gratis() {
    let page = file_page(200, &fixture("file_page.html"), &rules(), T0).unwrap();
    assert_eq!(page.fid, "735569057");
    assert_eq!((page.secs, page.secs_found), (60, true));
    assert_eq!(page.start_timer, "/download/AjaxStartTimer");
    assert_eq!(page.get_link, "/download/AjaxGetDownloadLink");
    assert_eq!(page.captcha, "/download/captcha");
    assert_eq!(page.name.as_deref(), Some("Relatorio Anual 2026.rar"));
    // o "MB" do site é MiB
    assert_eq!(page.size, Some(156_762_112));
}

#[test]
fn contador_ausente_vale_o_padrao_e_acima_do_teto_falha() {
    let html = fixture("file_page.html").replace("var secs = 60;", "");
    let page = file_page(200, &html, &rules(), T0).unwrap();
    assert_eq!((page.secs, page.secs_found), (60, false));
    let long = fixture("file_page.html").replace("var secs = 60;", "var secs = 900;");
    let res = file_page(200, &long, &rules(), T0);
    assert!(
        matches!(&res, Err(HostError::Changed(m)) if m.contains("900 s")),
        "{res:?}"
    );
    let no_fid = fixture("file_page.html").replace("var fid = 735569057;", "");
    assert!(matches!(
        file_page(200, &no_fid, &rules(), T0),
        Err(HostError::Changed(_))
    ));
}

#[test]
fn esperas_do_servidor() {
    let wait = |name: &str| match file_page(200, &fixture(name), &rules(), T0) {
        Err(HostError::Wait { until, reason }) => {
            (until.duration_since(T0).unwrap().as_secs(), reason)
        }
        other => panic!("{name}: {other:?}"),
    };
    assert_eq!(
        wait("file_delay.html"),
        (120 * 60 + 5, WaitReason::HostLimit)
    );
    assert_eq!(
        wait("file_hourly.html"),
        (120 * 60 + 5, WaitReason::HostLimit)
    );
    assert_eq!(
        wait("file_one_at_a_time.html"),
        (10 * 60 + 5, WaitReason::HostLimit)
    );
    // diário: até a próxima meia-noite UTC (T0 = 00:00 de 1/1/1970)
    assert_eq!(
        wait("file_daily.html"),
        (86_400 + 300, WaitReason::HostLimit)
    );
}

#[test]
fn so_premium_tamanho_offline_e_cloudflare() {
    let res = file_page(200, &fixture("file_premium_only.html"), &rules(), T0);
    assert!(
        matches!(&res, Err(HostError::PremiumOnly(Some(m))) if m.contains("premium")),
        "{res:?}"
    );
    let res = file_page(200, &fixture("file_size_limit.html"), &rules(), T0);
    assert!(
        matches!(&res, Err(HostError::PremiumOnly(Some(m))) if m.contains("500 MB")),
        "{res:?}"
    );
    assert_eq!(
        file_page(200, &fixture("file_404.html"), &rules(), T0),
        Err(HostError::Offline)
    );
    assert_eq!(
        file_page(404, "<html></html>", &rules(), T0),
        Err(HostError::Offline)
    );
    let res = file_page(503, &fixture("cloudflare.html"), &rules(), T0);
    assert!(
        matches!(&res, Err(HostError::BrowserRequired(m)) if m.contains("Rapidgator") && m.contains("navegador")),
        "{res:?}"
    );
    // marcador do Cloudflare numa página normal (200) não é desafio
    let page = fixture("file_page.html").replace(
        "</body>",
        "<script src='/cdn-cgi/challenge-platform/x.js'></script>cf_chl_opt</body>",
    );
    assert!(file_page(200, &page, &rules(), T0).is_ok());
}

#[test]
fn respostas_do_contador() {
    let sid = timer(200, &fixture("timer_started.json"), &rules(), T0).unwrap();
    assert_eq!(sid, "fakesid00rapidgator000000000000");
    assert_eq!(
        link_ready(200, &fixture("link_done.json"), &rules(), T0),
        Ok(())
    );
    let backoff = |r: Result<(), HostError>| {
        matches!(
            r,
            Err(HostError::Wait {
                reason: WaitReason::Backoff,
                ..
            })
        )
    };
    assert!(backoff(link_ready(
        200,
        &fixture("link_error.json"),
        &rules(),
        T0
    )));
    assert!(backoff(
        timer(200, r#"{"state":"error"}"#, &rules(), T0).map(|_| ())
    ));
    // página no lugar do JSON: as situações de sempre valem
    let res = timer(200, &fixture("file_hourly.html"), &rules(), T0);
    assert!(matches!(
        res,
        Err(HostError::Wait {
            reason: WaitReason::HostLimit,
            ..
        })
    ));
    assert!(matches!(
        timer(200, "<html>?</html>", &rules(), T0),
        Err(HostError::Changed(_))
    ));
}

#[test]
fn pagina_do_captcha_turnstile() {
    let page = url("https://rapidgator.net/download/captcha");
    for name in ["captcha_turnstile.html", "captcha_turnstile_script.html"] {
        let CaptchaPage::Form { form, kind } =
            captcha_page(200, &fixture(name), &page, &rules(), T0).unwrap()
        else {
            panic!("{name}: esperava o formulário");
        };
        assert_eq!(
            kind,
            CaptchaKind::Turnstile {
                site_key: "0x4AAAAAAAFAKEsitekeyFAKE".into()
            },
            "{name}"
        );
        assert_eq!(form.target(&page).path(), "/download/captcha");
        assert!(
            form.fields
                .contains(&("YII_CSRF_TOKEN".into(), "fakecsrf0123456789".into()))
        );
    }
    let no_key = fixture("captcha_turnstile_script.html").replace("sitekey:", "chave:");
    assert!(matches!(
        captcha_page(200, &no_key, &page, &rules(), T0),
        Err(HostError::Changed(_))
    ));
    // link já pronto, sem captcha
    let res = captcha_page(200, &fixture("final_link.html"), &page, &rules(), T0).unwrap();
    assert!(matches!(res, CaptchaPage::Link(u) if u.host_str() == Some("pr12.rapidgator.net")));
}

#[test]
fn pagina_final_link_ou_de_novo() {
    let page = url("https://rapidgator.net/download/captcha");
    let Final::Link(link) =
        final_page(200, &fixture("final_link.html"), &page, &rules(), T0).unwrap()
    else {
        panic!("sem link");
    };
    assert_eq!(
        link.query(),
        Some("r=download/index&session_id=FAKEsession0000000000")
    );
    let alt = "<script>return 'https://pr_srv.rapidgator.net//?r=download/index&session_id=Abc123';</script>";
    assert!(matches!(
        final_page(200, alt, &page, &rules(), T0),
        Ok(Final::Link(_))
    ));
    let href = "<script>location.href = 'https://pr3.rapidgator.net/download/a.rar';</script>";
    assert!(matches!(
        final_page(200, href, &page, &rules(), T0),
        Ok(Final::Link(_))
    ));
    assert_eq!(
        final_page(200, &fixture("captcha_wrong.html"), &page, &rules(), T0),
        Ok(Final::Again)
    );
    assert_eq!(
        final_page(200, &fixture("captcha_turnstile.html"), &page, &rules(), T0),
        Ok(Final::Again)
    );
    assert!(matches!(
        final_page(200, "<html>nada</html>", &page, &rules(), T0),
        Err(HostError::Changed(_))
    ));
    let js = "<script>return 'javascript:alert(1)';</script>";
    assert!(matches!(
        final_page(200, js, &page, &rules(), T0),
        Err(HostError::Changed(_))
    ));
}
