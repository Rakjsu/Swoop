//! Rapidgator falso, segunda metade: página do captcha, resposta (CSRF,
//! prazo e o token nos dois campos) e o link final de uso único.

use crate::file::Shared;
use crate::knobs::Knobs;
use crate::rapidgator::{Flow, Inner};
use crate::rapidgator_pages as pages;
use axum::Form;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Fluxo da sessão com o link liberado (o mais recente).
fn ready_flow(g: &mut Inner) -> Option<(String, &mut Flow)> {
    g.flows
        .iter_mut()
        .filter(|(_, f)| f.link_ok)
        .max_by_key(|(_, f)| f.started)
        .map(|(sid, f)| (sid.clone(), f))
}

/// Página do captcha (ou o link direto, com `captcha=none`).
pub async fn captcha_page(State(st): State<Shared>, headers: HeaderMap) -> Response {
    let mut g = st.rg.0.lock().unwrap();
    g.stats.captcha_pages += 1;
    let Some((sid, flow)) = ready_flow(&mut g) else {
        return pages::html(
            StatusCode::OK,
            "<html><body>Download session expired</body></html>".into(),
        );
    };
    flow.captcha_shown = Some(Instant::now());
    let (knobs, csrf) = (flow.knobs.clone(), flow.csrf.clone());
    if knobs.captcha == "none" {
        return deliver(&mut g, &sid, &headers);
    }
    pages::html(
        StatusCode::OK,
        pages::captcha(&csrf, knobs.captcha == "script", false),
    )
}

/// Resposta do captcha: confere CSRF, prazo e o token nos dois campos.
pub async fn captcha_submit(
    State(st): State<Shared>,
    headers: HeaderMap,
    Form(fields): Form<HashMap<String, String>>,
) -> Response {
    let mut g = st.rg.0.lock().unwrap();
    g.stats.submits += 1;
    let Some((sid, flow)) = ready_flow(&mut g) else {
        return pages::html(
            StatusCode::OK,
            "<html><body>Download session expired</body></html>".into(),
        );
    };
    let late = flow
        .captcha_shown
        .is_none_or(|t| t.elapsed() > Duration::from_millis(flow.knobs.deadline_ms));
    let csrf_ok = fields.get("YII_CSRF_TOKEN") == Some(&flow.csrf);
    let answer_ok = ["DownloadCaptchaForm[verifyCode]", "cf-turnstile-response"]
        .iter()
        .all(|k| fields.get(*k).map(String::as_str) == Some("ok"));
    let (csrf, script) = (flow.csrf.clone(), flow.knobs.captcha == "script");
    if late {
        g.stats.late_submits += 1;
        g.flows.remove(&sid);
        g.open.remove(&sid);
        let cookie = [(header::SET_COOKIE, "failed_on_captcha=1; Path=/")];
        return (
            cookie,
            pages::html(
                StatusCode::OK,
                "<html><body>You have reached your hourly downloads limit.</body></html>".into(),
            ),
        )
            .into_response();
    }
    if !(csrf_ok && answer_ok) {
        g.stats.wrong_answers += 1;
        return pages::html(StatusCode::OK, pages::captcha(&csrf, script, true));
    }
    deliver(&mut g, &sid, &headers)
}

/// Entrega o link de uso único do fluxo `sid`.
fn deliver(g: &mut Inner, sid: &str, headers: &HeaderMap) -> Response {
    let Some(flow) = g.flows.remove(sid) else {
        return pages::html(StatusCode::OK, "<html></html>".into());
    };
    g.next += 1;
    let token = format!("tok{}", g.next);
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("127.0.0.1");
    let link = format!("http://{host}/dl/{token}/{}", flow.name);
    g.tokens.insert(token, (flow.knobs.clone(), flow.name));
    g.stats.links += 1;
    g.last_download = Some(Instant::now());
    pages::html(
        StatusCode::OK,
        pages::final_link(&link, flow.knobs.final_page == "href"),
    )
}

/// Link final: uso único, User-Agent de navegador (o da sessão).
pub async fn download(
    State(st): State<Shared>,
    headers: HeaderMap,
    Path((token, name)): Path<(String, String)>,
) -> Response {
    let entry = {
        let mut g = st.rg.0.lock().unwrap();
        g.stats.final_link_gets += 1;
        let ua = headers
            .get(header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        match g.tokens.remove(&token) {
            Some(e) if !ua.starts_with("Swoop/") => Some(e),
            _ => {
                g.stats.final_link_rejected += 1;
                None
            }
        }
    };
    let Some((knobs, _)) = entry else {
        return (StatusCode::GONE, "File is already downloading").into_response();
    };
    st.rg.0.lock().unwrap().open.clear();
    let file = Knobs {
        size: knobs.size,
        seed: knobs.seed,
        norange: 1,
        cd: 1,
        ..Knobs::default()
    };
    crate::file::serve(State(st), Path(name), Query(file), headers).await
}
