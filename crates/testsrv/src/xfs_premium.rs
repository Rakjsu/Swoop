//! Conta premium do XFileSharing falso: API com chave, login com sessão e a
//! página do arquivo vista com a sessão premium.
//!
//! - `GET /api/file/direct_link?key=&file_code=` e `GET /api/account/info?key=`:
//!   `KEY` é premium, `FREE_KEY` é conta grátis, o resto é recusado;
//! - `POST /` com `op=login`: `USER`/`PASS` entram (cookie de sessão);
//! - `GET /?op=my_account`: validade do premium;
//! - `GET /{code}` com a sessão: formulário premium (`download2` +
//!   `method_premium`), que leva ao arquivo sem contador nem captcha.
//!
//! O arquivo entregue ao premium segue `XfsState::set_premium_file`.

use crate::xfs::XfsState;
use axum::Form;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;

pub const KEY: &str = "chave-premium";
pub const FREE_KEY: &str = "chave-gratis";
pub const USER: &str = "ana";
pub const PASS: &str = "senha-premium";
const SESSION: &str = "xfss=sessao-premium";

#[derive(Deserialize)]
pub struct ApiQuery {
    key: Option<String>,
    file_code: Option<String>,
}

/// Recusa (status 403 no JSON, como a API do XFS).
fn refused() -> Response {
    axum::Json(json!({ "msg": "Invalid key", "status": 403 })).into_response()
}

/// `GET /api/file/direct_link`.
pub async fn direct_link(
    State(st): State<XfsState>,
    headers: HeaderMap,
    Query(q): Query<ApiQuery>,
) -> Response {
    if q.key.as_deref() != Some(KEY) {
        return refused();
    }
    let code = q.file_code.unwrap_or_default();
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("127.0.0.1");
    st.count_premium_link();
    let url = format!("http://{host}{}", file_path(&st, &code));
    axum::Json(json!({ "msg": "OK", "status": 200, "result": { "url": url } })).into_response()
}

/// `GET /api/account/info`.
pub async fn account_info(Query(q): Query<ApiQuery>) -> Response {
    let expire = match q.key.as_deref() {
        Some(KEY) => "2099-01-01 00:00:00",
        Some(FREE_KEY) => "2020-01-01 00:00:00",
        _ => return refused(),
    };
    let result = json!({ "login": USER, "premium_expire": expire, "traffic_left": "102400" });
    axum::Json(json!({ "msg": "OK", "status": "200", "result": result })).into_response()
}

/// `POST /` (`op=login`).
pub async fn login(Form(f): Form<HashMap<String, String>>) -> Response {
    let get = |k: &str| f.get(k).map(String::as_str);
    if get("op") != Some("login") || get("login") != Some(USER) || get("password") != Some(PASS) {
        return Html("<div class=\"err\">Incorrect Login or Password</div>").into_response();
    }
    (
        [(header::SET_COOKIE, format!("{SESSION}; Path=/"))],
        Html("<a href=\"/?op=my_files\">My Files</a> <a href=\"/?op=logout\">Logout</a>"),
    )
        .into_response()
}

/// `GET /?op=my_account`.
pub async fn my_account(headers: HeaderMap) -> Response {
    if !session(&headers) {
        return Html("<form><input name=\"op\" value=\"login\"></form>").into_response();
    }
    Html(
        "<a href=\"/?op=logout\">Logout</a><table><tr><td>Premium account expire:</td>\
         <td><b>2099-01-01 00:00:00</b></td></tr></table>",
    )
    .into_response()
}

/// A requisição traz a sessão premium?
pub fn session(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .any(|v| v.split(';').any(|c| c.trim() == SESSION))
}

/// Página do arquivo com a sessão premium: só o formulário premium.
pub fn page(code: &str) -> Html<String> {
    Html(format!(
        "<html><body><h2>{code}.bin</h2><form name=\"F1\" method=\"POST\" action=\"\">\
         <input type=\"hidden\" name=\"op\" value=\"download2\">\
         <input type=\"hidden\" name=\"id\" value=\"{code}\">\
         <input type=\"hidden\" name=\"method_premium\" value=\"1\">\
         <input type=\"submit\" value=\"Download\"></form></body></html>"
    ))
}

/// Envio do formulário premium: direto para o arquivo.
pub fn redirect(st: &XfsState, code: &str) -> Response {
    st.count_premium_link();
    (StatusCode::FOUND, [(header::LOCATION, file_path(st, code))]).into_response()
}

fn file_path(st: &XfsState, code: &str) -> String {
    format!("/file/{code}.bin?{}", st.premium_file())
}
