//! HTML e JSON do Rapidgator falso, no formato das fixtures reconstruídas
//! (`crates/hosts/tests/fixtures/rapidgator`).

use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

/// Página HTML com status.
pub fn html(status: StatusCode, body: String) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body,
    )
        .into_response()
}

/// Resposta JSON do contador.
pub fn json(body: &str) -> Response {
    (
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        body.to_owned(),
    )
        .into_response()
}

/// Página do arquivo: nome, tamanho, aviso (`alert`) e as variáveis do
/// download grátis (sem `secs` com `hide_secs`).
pub fn file_page(name: &str, size: u64, alert: &str, fid: u64, secs: Option<u64>) -> String {
    let mib = size as f64 / (1024.0 * 1024.0);
    let secs = secs.map(|s| format!("var secs = {s};")).unwrap_or_default();
    let alert = if alert.is_empty() {
        String::new()
    } else {
        format!("<div class=\"alert\">{alert}</div>")
    };
    format!(
        "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <title>Download file {name}</title></head><body>\
         <div class=\"file-descr\"><p><strong>Downloading: </strong><a href=\"\">{name}</a></p>\
         <div>File size: <strong>{mib:.1} MB</strong></div></div>{alert}\
         <div class=\"btm\"><a class=\"btn-free\" href=\"javascript:void(0)\">SLOW SPEED DOWNLOAD</a></div>\
         <script>window.siteRan = true;\n\
         var startTimerUrl = '/download/AjaxStartTimer';\n\
         var getDownloadUrl = '/download/AjaxGetDownloadLink';\n\
         var captchaUrl = '/download/captcha';\n\
         var fid = {fid};\n{secs}\n</script></body></html>"
    )
}

/// Só premium.
pub fn premium_only() -> String {
    "<html><head><title>Download file pago.7z</title></head><body>\
     <div class=\"alert\">This file can be downloaded by premium only</div></body></html>"
        .into()
}

/// 404 do site.
pub fn not_found() -> String {
    "<html><head><title>404 File not found</title></head><body><h1>404 File not found</h1></body></html>"
        .into()
}

/// Desafio do Cloudflare no site inteiro.
pub fn cloudflare() -> String {
    "<html><head><title>Just a moment...</title></head><body>\
     <script>window._cf_chl_opt={cType:'managed'};</script></body></html>"
        .into()
}

/// Formulário do captcha (Turnstile), com a chave em `data-sitekey` ou num
/// script, e o aviso de resposta errada.
pub fn captcha(csrf: &str, key_in_script: bool, wrong: bool) -> String {
    let widget = if key_in_script {
        "<div class=\"cf-turnstile\"></div><script>turnstile.render('.cf-turnstile', \
         { sitekey: '0x4AAAAAAAFAKEsitekeyFAKE' });</script>"
    } else {
        "<div class=\"cf-turnstile\" data-sitekey=\"0x4AAAAAAAFAKEsitekeyFAKE\"></div>"
    };
    let error = if wrong {
        "<div class=\"errorSummary\">The verification code is incorrect.</div>"
    } else {
        ""
    };
    format!(
        "<html><head><title>Download file</title></head><body>{error}\
         <form id=\"captchaform\" action=\"/download/captcha\" method=\"post\">\
         <input type=\"hidden\" name=\"YII_CSRF_TOKEN\" value=\"{csrf}\">\
         <input type=\"hidden\" name=\"DownloadCaptchaForm[verifyCode]\" value=\"\">\
         {widget}<a id=\"submit-button\">Send</a></form></body></html>"
    )
}

/// Página final com o link (`return '…'` ou `location.href = '…'`).
pub fn final_link(link: &str, href: bool) -> String {
    let js = if href {
        format!("location.href = '{link}';")
    } else {
        format!("function getUrl() {{ return '{link}'; }}")
    };
    format!(
        "<html><head><title>Download file</title></head><body>\
         <a class=\"btn-download\" href=\"javascript:void(0)\">CLICK HERE TO DOWNLOAD</a>\
         <script>{js}</script></body></html>"
    )
}
