use super::*;
use crate::Rules;

fn rules() -> XfsRules {
    Rules::embedded().xfs
}

fn code_of(s: &str) -> Option<String> {
    code(&Url::parse(s).unwrap(), &rules())
}

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/xfs/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).unwrap()
}

fn page() -> Url {
    Url::parse("https://fastfile.cc/abcdefgh1234").unwrap()
}

const T0: SystemTime = SystemTime::UNIX_EPOCH;

#[test]
fn formatos_de_link() {
    let c = || Some("abcdefgh1234".to_owned());
    assert_eq!(code_of("https://fastfile.cc/abcdefgh1234"), c());
    assert_eq!(
        code_of("https://fastfile.cc/abcdefgh1234/Relatorio.pdf.html"),
        c()
    );
    assert_eq!(code_of("https://fastfile.cc/abcdefgh1234.html"), c());
    assert_eq!(code_of("https://www.fastfile.cc/abcdefgh1234"), c());
    assert_eq!(code_of("http://fastfile.cc/abcdefgh1234"), c());
    assert_eq!(code_of("https://fastfile.cc/embed-abcdefgh1234.html"), c());
    assert_eq!(code_of("https://fastfile.cc/d/abcdefgh1234"), c());
    assert_eq!(code_of("https://fastfile.cc/f/abcdefgh1234"), c());
    assert_eq!(code_of("https://fastfile.cc/file/abcdefgh1234"), c());
    assert_eq!(code_of("https://fastfile.cc/ABCDEFGH1234"), c());
    assert_eq!(
        code_of("https://katfile.com/abcdefgh1234/nome.zip.html"),
        c()
    );
    assert_eq!(code_of("https://ddownload.com/abcdefgh1234"), c());
    // não são arquivos
    assert_eq!(code_of("https://fastfile.cc/"), None);
    assert_eq!(code_of("https://fastfile.cc/premium.html"), None);
    assert_eq!(code_of("https://fastfile.cc/?op=registration"), None);
    assert_eq!(code_of("https://fastfile.cc/abc"), None);
    assert_eq!(
        code_of("https://fastfile.cc.evil.example/abcdefgh1234"),
        None
    );
    assert_eq!(code_of("https://outro.example/abcdefgh1234"), None);
}

#[test]
fn pagina_do_arquivo_pede_o_download_gratis() {
    let Step::Download1 { form, name, size } =
        step(&fixture("fastfile_page1.html"), &page(), &rules(), T0).unwrap()
    else {
        panic!("esperava a página do arquivo");
    };
    assert!(form.fields.contains(&("op".into(), "download1".into())));
    assert!(
        form.fields
            .contains(&("method_free".into(), "Free Download".into()))
    );
    assert!(!form.fields.iter().any(|(k, _)| k == "method_premium"));
    assert_eq!(form.target(&page()), page());
    assert_eq!(name.as_deref(), Some("Relatorio Anual.pdf"));
    assert_eq!(size, Some(13_107_200));
}

#[test]
fn download_gratis_com_recaptcha_e_contador() {
    let Step::Download2(free) =
        step(&fixture("fastfile_page2.html"), &page(), &rules(), T0).unwrap()
    else {
        panic!("esperava a página do download grátis");
    };
    assert_eq!(free.countdown_secs, 30);
    assert!(
        free.form
            .fields
            .contains(&("op".into(), "download2".into()))
    );
    assert!(
        free.form
            .fields
            .contains(&("rand".into(), "k3j4h5g6f7d8s9a0".into()))
    );
    assert!(!free.form.fields.iter().any(|(k, _)| k == "method_premium"));
    let (kind, field) = free.captcha.unwrap();
    assert_eq!(field, "g-recaptcha-response");
    assert!(
        matches!(kind, CaptchaKind::Recaptcha2 { site_key } if site_key.starts_with("6LcFAKE"))
    );
}

#[test]
fn captcha_de_imagem_e_de_digitos() {
    let katfile = Url::parse("https://katfile.com/abcdefgh1234").unwrap();
    let Step::Download2(free) =
        step(&fixture("katfile_page2.html"), &katfile, &rules(), T0).unwrap()
    else {
        panic!("katfile");
    };
    assert_eq!(free.countdown_secs, 60);
    assert_eq!(free.form.target(&katfile).path(), "/abcdefgh1234");
    let (kind, field) = free.captcha.unwrap();
    assert_eq!(field, "code");
    assert!(
        matches!(kind, CaptchaKind::Image { url } if url.as_str() == "https://katfile.com/captchas/fake0captcha0img.jpg")
    );

    let dd = Url::parse("https://ddownload.com/abcdefgh1234").unwrap();
    let Step::Download2(free) = step(&fixture("ddownload_page2.html"), &dd, &rules(), T0).unwrap()
    else {
        panic!("ddownload");
    };
    assert_eq!(free.countdown_secs, 0);
    let (kind, _) = free.captcha.unwrap();
    // o scraper reescreve `&#55;` como "7"
    assert!(
        matches!(kind, CaptchaKind::Html { html } if html.contains("padding-left:31px") && html.contains('7'))
    );
}

#[test]
fn espera_entre_downloads_vale_para_o_servidor() {
    let res = step(&fixture("fastfile_wait.html"), &page(), &rules(), T0);
    assert_eq!(
        res,
        Err(HostError::Wait {
            until: T0 + Duration::from_secs(5 * 60 + 12 + 5),
            reason: WaitReason::HostLimit,
        })
    );
}

#[test]
fn removido_premium_e_cloudflare() {
    assert_eq!(
        step(&fixture("fastfile_notfound.html"), &page(), &rules(), T0),
        Err(HostError::Offline)
    );
    assert_eq!(
        step(&fixture("fastfile_premium.html"), &page(), &rules(), T0),
        Err(HostError::PremiumOnly)
    );
    assert!(matches!(
        step(&fixture("fastfile_cloudflare.html"), &page(), &rules(), T0),
        Err(HostError::BrowserRequired(m)) if m.contains("Cloudflare")
    ));
}

#[test]
fn pagina_final_link_ou_de_novo() {
    assert_eq!(
        final_page(&fixture("fastfile_final.html"), &page(), &rules(), T0),
        Ok(Final::Link(
            Url::parse("https://s12.fastfile.cc/d/q8w7e6r5t4y3u2i1/Relatorio%20Anual.pdf").unwrap()
        ))
    );
    assert_eq!(
        final_page(
            &fixture("fastfile_wrong_captcha.html"),
            &page(),
            &rules(),
            T0
        ),
        Ok(Final::Again)
    );
    assert_eq!(
        final_page(&fixture("fastfile_page2.html"), &page(), &rules(), T0),
        Ok(Final::Again)
    );
    assert!(matches!(
        final_page("<html>nada</html>", &page(), &rules(), T0),
        Err(HostError::Changed(_))
    ));
}
