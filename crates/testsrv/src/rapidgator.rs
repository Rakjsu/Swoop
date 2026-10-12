//! Rapidgator falso (grátis), num endereço próprio do `TestServer`:
//! página `/file/{id}[/{nome}]` → `AjaxStartTimer` → contador →
//! `AjaxGetDownloadLink` → `/download/captcha` (Turnstile falso, resposta
//! certa = `ok`) → link `/dl/{token}/{nome}`, de uso único.
//!
//! Botões (query da página do arquivo): `secs` (padrão 2), `hide_secs=1`,
//! `captcha=turnstile|script|none`, `delay` (min de espera depois de cada
//! download), `hourly=1`, `daily=1`, `one_at_a_time=1`, `size_limit_mb`,
//! `premium_only=1`, `offline=1`, `cloudflare=1`, `final=js|href`,
//! `timer_error=1`, `deadline_ms` (prazo do captcha, padrão 100 000),
//! `broken=1` (página sem as variáveis do download grátis), `size`, `seed`. O link final recusa o User-Agent do cliente de download (`Swoop/`).

use crate::file::Shared;
use crate::rapidgator_pages as pages;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Botões de um arquivo do Rapidgator falso.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RgKnobs {
    pub secs: u64,
    pub hide_secs: u8,
    pub captcha: String,
    pub delay: u64,
    pub hourly: u8,
    pub daily: u8,
    pub one_at_a_time: u8,
    pub size_limit_mb: Option<u64>,
    pub premium_only: u8,
    pub offline: u8,
    pub cloudflare: u8,
    #[serde(rename = "final")]
    pub final_page: String,
    pub timer_error: u8,
    pub deadline_ms: u64,
    pub broken: u8,
    pub size: u64,
    pub seed: u64,
}

impl Default for RgKnobs {
    fn default() -> Self {
        Self {
            secs: 2,
            hide_secs: 0,
            captcha: "turnstile".into(),
            delay: 0,
            hourly: 0,
            daily: 0,
            one_at_a_time: 0,
            size_limit_mb: None,
            premium_only: 0,
            offline: 0,
            cloudflare: 0,
            final_page: "js".into(),
            timer_error: 0,
            deadline_ms: 100_000,
            broken: 0,
            size: 1 << 20,
            seed: 1,
        }
    }
}

/// O que o Rapidgator falso viu.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RgStats {
    pub timers: u64,
    /// Link pedido antes do fim do contador.
    pub early_link_calls: u64,
    /// Contador começado com outro download do grátis em andamento.
    pub parallel_starts: u64,
    pub captcha_pages: u64,
    pub submits: u64,
    pub wrong_answers: u64,
    /// Resposta enviada depois do prazo do captcha.
    pub late_submits: u64,
    /// GETs no link final (válidos ou não) e os recusados.
    pub final_link_gets: u64,
    pub final_link_rejected: u64,
    pub links: u64,
    /// API premium: logins (POST), logins com a senha na URL (nunca deve
    /// haver), links premium entregues.
    pub logins: u64,
    pub password_in_query: u64,
    pub premium_links: u64,
    /// Páginas de pasta lidas.
    pub folder_pages: u64,
}

/// Um fluxo grátis (por `sid` do contador).
pub(crate) struct Flow {
    pub(crate) knobs: RgKnobs,
    pub(crate) name: String,
    pub(crate) started: Instant,
    pub(crate) link_ok: bool,
    pub(crate) captcha_shown: Option<Instant>,
    pub(crate) csrf: String,
}

#[derive(Default)]
pub(crate) struct Inner {
    /// Arquivo aberto por último em cada sessão (cookie) → botões e nome.
    pages: HashMap<String, (RgKnobs, String)>,
    pub(crate) flows: HashMap<String, Flow>,
    /// Fluxos começados e ainda sem o link usado.
    pub(crate) open: HashSet<String>,
    /// Links finais ainda não usados → (botões, nome).
    pub(crate) tokens: HashMap<String, (RgKnobs, String)>,
    pub(crate) last_download: Option<Instant>,
    pub(crate) next: u64,
    pub(crate) stats: RgStats,
    /// API premium: tokens válidos → conta premium?
    pub(crate) api_tokens: HashMap<String, bool>,
    /// Arquivo que a conta premium baixa: (tamanho, semente, taxa).
    pub(crate) premium_file: Option<(u64, u64, Option<u64>)>,
    pub(crate) traffic_exceeded: bool,
}

/// Estado do Rapidgator falso.
#[derive(Clone, Default)]
pub struct RgState(pub(crate) Arc<Mutex<Inner>>);

impl RgState {
    pub fn stats(&self) -> RgStats {
        self.0.lock().unwrap().stats.clone()
    }

    /// Arquivo que a conta premium recebe (tamanho, semente, bytes/s por conexão).
    pub fn set_premium_file(&self, size: u64, seed: u64, rate: Option<u64>) {
        self.0.lock().unwrap().premium_file = Some((size, seed, rate));
    }

    /// Derruba todas as sessões da API (a próxima chamada recebe 401).
    pub fn expire_sessions(&self) {
        self.0.lock().unwrap().api_tokens.clear();
    }

    /// Tráfego premium esgotado (423).
    pub fn set_traffic_exceeded(&self, exceeded: bool) {
        self.0.lock().unwrap().traffic_exceeded = exceeded;
    }
}

/// Cookie de sessão do site (`PHPSESSID`), se veio.
fn session(headers: &HeaderMap) -> Option<String> {
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    cookies
        .split("; ")
        .find_map(|c| c.strip_prefix("PHPSESSID="))
        .map(str::to_owned)
}

/// Página do arquivo (cria a sessão se não houver).
pub async fn file_page(
    State(st): State<Shared>,
    headers: HeaderMap,
    Path(path): Path<Vec<String>>,
    Query(k): Query<RgKnobs>,
) -> Response {
    let name = path
        .get(1)
        .map_or("arquivo.bin", |n| n.trim_end_matches(".html"))
        .to_owned();
    let mut g = st.rg.0.lock().unwrap();
    g.next += 1;
    let (fid, sess) = (
        g.next,
        session(&headers).unwrap_or_else(|| format!("sess{}", g.next)),
    );
    let delayed = k.delay > 0
        && g.last_download
            .is_some_and(|t| t.elapsed() < Duration::from_secs(k.delay * 60));
    g.pages.insert(sess.clone(), (k.clone(), name.clone()));
    drop(g);
    let cookie = [(header::SET_COOKIE, format!("PHPSESSID={sess}; Path=/"))];
    let alert = match () {
        _ if delayed => format!(
            "Delay between downloads must be not less than {} min.",
            k.delay
        ),
        _ if k.hourly == 1 => {
            "You have reached your hourly downloads limit. Please, try again later.".into()
        }
        _ if k.daily == 1 => "You have reached your daily downloads limit".into(),
        _ if k.one_at_a_time == 1 => {
            "You can`t download more than 1 file at a time in free mode.".into()
        }
        _ => match k.size_limit_mb {
            Some(mb) if k.size > mb * 1024 * 1024 => {
                format!("You can download files up to {mb} MB in free mode")
            }
            _ => String::new(),
        },
    };
    let body = if k.offline == 1 {
        return (
            cookie,
            pages::html(StatusCode::NOT_FOUND, pages::not_found()),
        )
            .into_response();
    } else if k.cloudflare == 1 {
        return pages::html(StatusCode::SERVICE_UNAVAILABLE, pages::cloudflare());
    } else if k.premium_only == 1 {
        pages::premium_only()
    } else if k.broken == 1 {
        format!(
            "<html><head><title>Download file {name}</title></head><body>tema novo</body></html>"
        )
    } else {
        let secs = (k.hide_secs == 0).then_some(k.secs);
        pages::file_page(&name, k.size, &alert, fid, secs)
    };
    (cookie, pages::html(StatusCode::OK, body)).into_response()
}

#[derive(Deserialize)]
pub struct Fid {
    #[allow(dead_code)]
    fid: String,
}

/// Começa o contador da sessão.
pub async fn start_timer(
    State(st): State<Shared>,
    headers: HeaderMap,
    Query(_): Query<Fid>,
) -> Response {
    let Some((knobs, name)) =
        session(&headers).and_then(|s| st.rg.0.lock().unwrap().pages.get(&s).cloned())
    else {
        return pages::json(r#"{"state":"error","code":"session"}"#);
    };
    let busy = st.counters.snapshot().active_connections > 0;
    let mut g = st.rg.0.lock().unwrap();
    g.stats.timers += 1;
    if busy || !g.open.is_empty() {
        g.stats.parallel_starts += 1;
    }
    g.next += 1;
    let sid = format!("sid{:028}", g.next);
    let csrf = format!("csrf{}", g.next);
    g.open.insert(sid.clone());
    g.flows.insert(
        sid.clone(),
        Flow {
            knobs,
            name,
            started: Instant::now(),
            link_ok: false,
            captcha_shown: None,
            csrf,
        },
    );
    pages::json(&format!(r#"{{"state":"started","sid":"{sid}"}}"#))
}

#[derive(Deserialize)]
pub struct Sid {
    sid: String,
}

/// Pede o link: só depois do contador.
pub async fn get_link(State(st): State<Shared>, Query(q): Query<Sid>) -> Response {
    let mut g = st.rg.0.lock().unwrap();
    let Some((secs, timer_error, started)) = g
        .flows
        .get(&q.sid)
        .map(|f| (f.knobs.secs, f.knobs.timer_error, f.started))
    else {
        return pages::json(r#"{"state":"error"}"#);
    };
    if timer_error == 1 || started.elapsed() < Duration::from_secs(secs) {
        if timer_error == 0 {
            g.stats.early_link_calls += 1;
        }
        return pages::json(
            r#"{"state":"error","code":"You didn`t wait specified time. Try again or contact to administrator","0":"step3"}"#,
        );
    }
    g.flows.get_mut(&q.sid).unwrap().link_ok = true;
    pages::json(r#"{"state":"done"}"#)
}
