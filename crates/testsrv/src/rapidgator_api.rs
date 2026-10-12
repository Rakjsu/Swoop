//! API v2 falsa do Rapidgator (conta premium): login só por POST
//! (`cliente@exemplo.com` premium e `gratis@exemplo.com` grátis, senha
//! `senha-certa`), `user/info`, `file/info` (id começando com `ffff` = pago)
//! e `file/download`, que entrega `/premium/{id}/{nome}` com Range.

use crate::content::{effective_seed, md5_hex};
use crate::file::Shared;
use crate::knobs::Knobs;
use crate::rapidgator_pages as pages;
use axum::Form;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, header};
use axum::response::Response;
use std::collections::HashMap;
use std::sync::atomic::Ordering;

/// Senha das contas falsas.
pub const PASSWORD: &str = "senha-certa";
/// Conta premium e conta grátis.
pub const PREMIUM_USER: &str = "cliente@exemplo.com";
pub const FREE_USER: &str = "gratis@exemplo.com";

type Params = Query<HashMap<String, String>>;

/// Envelope `{"response": …, "status": …, "details": …}`.
fn envelope(status: u16, response: &str, details: &str) -> Response {
    let details = if details.is_empty() {
        "null".into()
    } else {
        format!("\"{details}\"")
    };
    pages::json(&format!(
        r#"{{"response":{response},"status":{status},"details":{details}}}"#
    ))
}

fn user_json(premium: bool) -> String {
    let (email, end, left) = if premium {
        (PREMIUM_USER, "4102444800", "4105663543401")
    } else {
        (FREE_USER, "null", "null")
    };
    format!(
        r#"{{"email":"{email}","is_premium":{premium},"premium_end_time":{end},"state":1,"traffic":{{"total":null,"left":{left}}}}}"#
    )
}

/// Login: só POST com `login` e `password` no corpo.
pub async fn login(
    State(st): State<Shared>,
    Query(q): Params,
    Form(f): Form<HashMap<String, String>>,
) -> Response {
    let mut g = st.rg.0.lock().unwrap();
    g.stats.logins += 1;
    if q.contains_key("password") {
        g.stats.password_in_query += 1;
    }
    let premium = match (
        f.get("login").map(String::as_str),
        f.get("password").map(String::as_str),
    ) {
        (Some(PREMIUM_USER), Some(PASSWORD)) => true,
        (Some(FREE_USER), Some(PASSWORD)) => false,
        _ => return envelope(401, "null", "Error: Wrong e-mail or password."),
    };
    g.next += 1;
    let token = format!("tok-api-{}", g.next);
    g.api_tokens.insert(token.clone(), premium);
    envelope(
        200,
        &format!(r#"{{"token":"{token}","user":{}}}"#, user_json(premium)),
        "",
    )
}

/// Conta do token.
fn token_premium(st: &Shared, q: &HashMap<String, String>) -> Option<bool> {
    let g = st.rg.0.lock().unwrap();
    q.get("token").and_then(|t| g.api_tokens.get(t)).copied()
}

pub async fn user_info(State(st): State<Shared>, Query(q): Params) -> Response {
    match token_premium(&st, &q) {
        Some(premium) => envelope(200, &format!(r#"{{"user":{}}}"#, user_json(premium)), ""),
        None => envelope(401, "null", "Error. Session doesn't exist"),
    }
}

/// Tamanho, semente e taxa do arquivo premium.
fn premium_file(st: &Shared) -> (u64, u64, Option<u64>) {
    st.rg
        .0
        .lock()
        .unwrap()
        .premium_file
        .unwrap_or((1 << 20, 1, None))
}

pub async fn file_info(State(st): State<Shared>, Query(q): Params) -> Response {
    if token_premium(&st, &q).is_none() {
        return envelope(401, "null", "Error. Session doesn't exist");
    }
    let id = q.get("file_id").cloned().unwrap_or_default();
    if id.starts_with("ffff") {
        return envelope(
            200,
            r#"{"file":{"file_id":"x","mode":2,"mode_label":"Paid","name":"pago.zip","hash":"","size":1}}"#,
            "",
        );
    }
    let (size, seed, _) = premium_file(&st);
    let md5 = md5_hex(
        effective_seed(seed, st.generation.load(Ordering::SeqCst)),
        size,
    );
    envelope(
        200,
        &format!(
            r#"{{"file":{{"file_id":"{id}","mode":0,"mode_label":"Public","name":"premium.bin","hash":"{md5}","size":"{size}"}}}}"#
        ),
        "",
    )
}

pub async fn file_download(
    State(st): State<Shared>,
    headers: HeaderMap,
    Query(q): Params,
) -> Response {
    if token_premium(&st, &q).is_none() {
        return envelope(401, "null", "Error. Session doesn't exist");
    }
    let mut g = st.rg.0.lock().unwrap();
    if g.traffic_exceeded {
        return envelope(423, "null", "Error: Exceeded traffic");
    }
    g.stats.premium_links += 1;
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("127.0.0.1");
    let id = q.get("file_id").cloned().unwrap_or_default();
    envelope(
        200,
        &format!(r#"{{"download_url":"http://{host}/premium/{id}/premium.bin","delay":0}}"#),
        "",
    )
}

/// Arquivo premium, com Range (várias conexões e retomada).
pub async fn premium_download(
    State(st): State<Shared>,
    headers: HeaderMap,
    Path((_, name)): Path<(String, String)>,
) -> Response {
    let (size, seed, rate) = premium_file(&st);
    let file = Knobs {
        size,
        seed,
        rate,
        ..Knobs::default()
    };
    crate::file::serve(State(st), Path(name), Query(file), headers).await
}
