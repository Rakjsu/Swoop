//! XFileSharing falso para testar o plugin e o fluxo do captcha sem rede.
//!
//! - `GET /{code}` → página do arquivo (formulário `op=download1`);
//! - `POST op=download1` → contador + captcha + formulário `F1` (`op=download2`),
//!   ou "espere até o próximo download", ou "só premium";
//! - `POST op=download2` → confere o contador e a resposta (`ok`) e
//!   redireciona para `/file/{nome}` (o arquivo determinístico de sempre).
//!
//! Botões (query): `countdown` (s, padrão 2), `wait_between` (s depois de
//! cada link entregue), `premium_only=1`, `captcha=recaptcha|image|none`,
//! `name`, `size`, `seed`.

use axum::Form;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Botões de uma página do XFS falso.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct XfsKnobs {
    pub countdown: u64,
    pub wait_between: u64,
    pub premium_only: u8,
    pub captcha: String,
    pub name: Option<String>,
    pub size: u64,
    pub seed: u64,
}

impl Default for XfsKnobs {
    fn default() -> Self {
        Self {
            countdown: 2,
            wait_between: 0,
            premium_only: 0,
            captcha: "recaptcha".into(),
            name: None,
            size: 1 << 20,
            seed: 1,
        }
    }
}

/// O que o XFS falso viu (para os testes conferirem).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct XfsStats {
    /// `download2` antes do fim do contador.
    pub early_submits: u64,
    /// Resposta de captcha errada.
    pub wrong_answers: u64,
    /// Links entregues.
    pub links: u64,
}

#[derive(Default)]
struct Inner {
    /// `rand` do formulário → quando o contador começou.
    issued: HashMap<String, Instant>,
    blocked_until: Option<Instant>,
    next_rand: u64,
    stats: XfsStats,
}

/// Estado compartilhado do XFS falso.
#[derive(Clone, Default)]
pub struct XfsState(Arc<Mutex<Inner>>);

impl XfsState {
    pub fn stats(&self) -> XfsStats {
        self.0.lock().unwrap().stats.clone()
    }
}

fn file_name(code: &str, k: &XfsKnobs) -> String {
    k.name.clone().unwrap_or_else(|| format!("{code}.bin"))
}

/// Página do arquivo.
pub async fn page(Path(code): Path<String>, Query(k): Query<XfsKnobs>) -> Html<String> {
    let name = file_name(&code, &k);
    Html(format!(
        "<html><body><h2>{name}</h2><span>({} KB)</span>\
         <form method=\"POST\" action=\"\">\
         <input type=\"hidden\" name=\"op\" value=\"download1\">\
         <input type=\"hidden\" name=\"id\" value=\"{code}\">\
         <input type=\"hidden\" name=\"fname\" value=\"{name}\">\
         <input type=\"submit\" name=\"method_free\" value=\"Free Download\">\
         <input type=\"submit\" name=\"method_premium\" value=\"Premium Download\">\
         </form></body></html>",
        k.size / 1024
    ))
}

/// Envio de formulário (`download1` ou `download2`).
pub async fn form(
    State(st): State<XfsState>,
    Path(code): Path<String>,
    Query(k): Query<XfsKnobs>,
    Form(fields): Form<HashMap<String, String>>,
) -> Response {
    let mut g = st.0.lock().unwrap();
    match fields.get("op").map(String::as_str) {
        Some("download1") => download1(&mut g, &code, &k),
        Some("download2") => download2(&mut g, &code, &k, &fields),
        _ => (StatusCode::BAD_REQUEST, "op?").into_response(),
    }
}

fn download1(g: &mut Inner, code: &str, k: &XfsKnobs) -> Response {
    if k.premium_only == 1 {
        return Html("<div>This file is available for Premium Users only.</div>").into_response();
    }
    if let Some(until) = g.blocked_until
        && let Some(left) = until.checked_duration_since(Instant::now())
    {
        let secs = left.as_secs() + 1;
        return Html(format!(
            "<div>You have to wait {} minutes, {} seconds till next download</div>",
            secs / 60,
            secs % 60
        ))
        .into_response();
    }
    g.next_rand += 1;
    let rand = format!("r{:015}", g.next_rand);
    g.issued.insert(rand.clone(), Instant::now());
    Html(free_page(code, k, &rand, "")).into_response()
}

fn free_page(code: &str, k: &XfsKnobs, rand: &str, note: &str) -> String {
    let captcha = match k.captcha.as_str() {
        "recaptcha" => "<div class=\"g-recaptcha\" data-sitekey=\"test-site-key\"></div>",
        "image" => "<img src=\"/captchas/test.jpg\"><input type=\"text\" name=\"code\">",
        _ => "",
    };
    format!(
        "<html><body>{note}<div>Wait <span class=\"seconds\">{}</span> seconds</div>\
         <form name=\"F1\" method=\"POST\" action=\"\">\
         <input type=\"hidden\" name=\"op\" value=\"download2\">\
         <input type=\"hidden\" name=\"id\" value=\"{code}\">\
         <input type=\"hidden\" name=\"rand\" value=\"{rand}\">\
         <input type=\"hidden\" name=\"method_free\" value=\"Free Download\">\
         {captcha}</form></body></html>",
        k.countdown
    )
}

fn download2(
    g: &mut Inner,
    code: &str,
    k: &XfsKnobs,
    fields: &HashMap<String, String>,
) -> Response {
    let rand = fields.get("rand").cloned().unwrap_or_default();
    let Some(issued) = g.issued.get(&rand).copied() else {
        // Sessão desconhecida: volta para a página do arquivo.
        return Html("<form><input type=\"hidden\" name=\"op\" value=\"download1\"></form>")
            .into_response();
    };
    if issued.elapsed() < Duration::from_secs(k.countdown) {
        g.stats.early_submits += 1;
        return Html("<div>Skipped countdown</div>").into_response();
    }
    let answer = fields
        .get("g-recaptcha-response")
        .or_else(|| fields.get("code"))
        .map(String::as_str);
    if k.captcha != "none" && answer != Some("ok") {
        g.stats.wrong_answers += 1;
        return Html(free_page(code, k, &rand, "<div>Wrong captcha</div>")).into_response();
    }
    g.issued.remove(&rand);
    g.stats.links += 1;
    if k.wait_between > 0 {
        g.blocked_until = Some(Instant::now() + Duration::from_secs(k.wait_between));
    }
    let target = format!(
        "/file/{}?size={}&seed={}",
        file_name(code, k),
        k.size,
        k.seed
    );
    (StatusCode::FOUND, [(header::LOCATION, target)]).into_response()
}
