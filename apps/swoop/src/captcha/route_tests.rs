use super::*;

fn site() -> Url {
    Url::parse("https://www.fastfile.cc/abcdefgh1234").unwrap()
}

fn go(s: &str) -> Route {
    route(&Url::parse(s).unwrap(), &site(), 7)
}

#[test]
fn site_e_provedores_abrem() {
    assert_eq!(go("https://fastfile.cc/abcdefgh1234"), Route::Allow);
    assert_eq!(go("https://www.fastfile.cc/"), Route::Allow);
    assert_eq!(go("https://s12.fastfile.cc/captchas/x.jpg"), Route::Allow);
    assert_eq!(
        go("https://www.google.com/recaptcha/api2/anchor?k=x"),
        Route::Allow
    );
    assert_eq!(
        go("https://www.recaptcha.net/recaptcha/api2/bframe"),
        Route::Allow
    );
    assert_eq!(
        go("https://newassets.hcaptcha.com/captcha/v1/x"),
        Route::Allow
    );
    assert_eq!(
        go("https://challenges.cloudflare.com/cdn-cgi/challenge-platform/x"),
        Route::Allow
    );
    assert_eq!(go("about:blank"), Route::Allow);
    assert_eq!(go("about:srcdoc"), Route::Allow);
}

#[test]
fn hosts_alheios_sao_negados() {
    assert_eq!(go("https://anuncio.example/pop"), Route::Deny);
    assert_eq!(go("https://fastfile.cc.evil.example/"), Route::Deny);
    assert_eq!(go("https://evilfastfile.cc/"), Route::Deny);
    assert_eq!(go("https://accounts.google.com/signin"), Route::Deny);
    assert_eq!(go("https://www.google.com/search?q=x"), Route::Deny);
    assert_eq!(
        go("https://google.com.evil.example/recaptcha/"),
        Route::Deny
    );
    // provedor só por https; o site não cai de https para http
    assert_eq!(
        go("http://www.google.com/recaptcha/api2/anchor"),
        Route::Deny
    );
    assert_eq!(go("http://fastfile.cc/abcdefgh1234"), Route::Deny);
    assert_eq!(go("https://user:pw@fastfile.cc/"), Route::Deny);
    assert_eq!(go("javascript:alert(1)"), Route::Deny);
    assert_eq!(go("data:text/html,<b>x</b>"), Route::Deny);
    assert_eq!(go("file:///C:/Windows/win.ini"), Route::Deny);
}

#[test]
fn resposta_so_pelo_endereco_reservado_e_do_download_certo() {
    assert_eq!(
        go("https://swoop-captcha.invalid/done?id=7&token=03AF%2Bx"),
        Route::Token("03AF+x".into())
    );
    assert_eq!(
        go("https://swoop-captcha.invalid/cancel?id=7"),
        Route::Close
    );
    // outro download, sem token, porta, http, caminho estranho
    assert_eq!(
        go("https://swoop-captcha.invalid/done?id=8&token=x"),
        Route::Deny
    );
    assert_eq!(
        go("https://swoop-captcha.invalid/done?id=7&token="),
        Route::Deny
    );
    assert_eq!(go("https://swoop-captcha.invalid/done?id=7"), Route::Deny);
    assert_eq!(
        go("https://swoop-captcha.invalid:444/done?id=7&token=x"),
        Route::Deny
    );
    assert_eq!(
        go("http://swoop-captcha.invalid/done?id=7&token=x"),
        Route::Deny
    );
    assert_eq!(
        go("https://swoop-captcha.invalid/outra?id=7&token=x"),
        Route::Deny
    );
    // imitações do endereço reservado em outro host
    assert_eq!(
        go("https://swoop-captcha.invalid.evil.example/done?id=7&token=x"),
        Route::Deny
    );
    assert_eq!(
        go("https://evil.example/swoop-captcha.invalid/done?id=7&token=x"),
        Route::Deny
    );
}

#[test]
fn site_em_http_local_do_teste() {
    let local = Url::parse("http://127.0.0.1:4000/abcdefgh1234").unwrap();
    let go = |s: &str| route(&Url::parse(s).unwrap(), &local, 1);
    assert_eq!(go("http://127.0.0.1:4000/abcdefgh1234"), Route::Allow);
    assert_eq!(go("http://127.0.0.2/"), Route::Deny);
}
