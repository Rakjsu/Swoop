//! Rota `/file/{nome}`: serve o arquivo determinístico com Range, ETag,
//! If-Range e os botões de falha de `Knobs`.

use crate::content::{effective_seed, fill};
use crate::knobs::{Knobs, parse_range};
use crate::stats::{ConnGuard, Counters};
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Estado compartilhado do servidor.
#[derive(Clone)]
pub struct Shared {
    pub counters: Counters,
    pub generation: Arc<AtomicU64>,
    pub requests: Arc<AtomicU64>,
}

/// Atende um arquivo (inteiro ou faixa).
pub async fn serve(
    State(st): State<Shared>,
    Path(name): Path<String>,
    Query(k): Query<Knobs>,
    headers: HeaderMap,
) -> Response {
    let generation = st.generation.load(Ordering::SeqCst);
    let seed = effective_seed(k.seed, generation);
    let etag = format!("\"s{}-g{}-n{}\"", k.seed, generation, k.size);

    if k.valid_until.is_some_and(|until| now_ms() > until) {
        return (StatusCode::FORBIDDEN, "link expirado").into_response();
    }
    let n = st.requests.fetch_add(1, Ordering::SeqCst);
    if k.fail > 0.0 && (n.wrapping_mul(2_654_435_761) % 1000) < (k.fail * 1000.0) as u64 {
        return (StatusCode::SERVICE_UNAVAILABLE, "falha simulada").into_response();
    }
    if k.size == 0 {
        return StatusCode::OK.into_response();
    }

    let range_header = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let if_range_ok = headers
        .get(header::IF_RANGE)
        .is_none_or(|v| v.as_bytes() == etag.as_bytes());
    let range = match (k.norange == 0, range_header) {
        (true, Some(h)) => match parse_range(h, k.size) {
            Some(r) => if_range_ok.then_some(r),
            None => return unsatisfiable(k.size),
        },
        _ => None,
    };

    let (status, start, end) = match range {
        Some((a, b)) => (StatusCode::PARTIAL_CONTENT, a, b + 1),
        None => (StatusCode::OK, 0, k.size),
    };

    let mut resp = Response::new(Body::from_stream(body_stream(BodyState {
        seed,
        pos: start,
        end,
        rate: k.rate_for(start),
        drop_after: k.drop_after,
        sent: 0,
        t0: Instant::now(),
        dropped: false,
        counters: st.counters.clone(),
        _guard: st.counters.open(&name),
    })));
    *resp.status_mut() = status;
    let h = resp.headers_mut();
    h.insert(header::CONTENT_LENGTH, (end - start).into());
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    if let Ok(v) = HeaderValue::from_str(&etag) {
        h.insert(header::ETAG, v);
    }
    let modified = UNIX_EPOCH + Duration::from_secs(1_700_000_000 + generation);
    if let Ok(v) = HeaderValue::from_str(&httpdate::fmt_http_date(modified)) {
        h.insert(header::LAST_MODIFIED, v);
    }
    if k.norange == 0 {
        h.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    }
    if status == StatusCode::PARTIAL_CONTENT {
        let cr = format!("bytes {}-{}/{}", start, end - 1, k.size);
        if let Ok(v) = HeaderValue::from_str(&cr) {
            h.insert(header::CONTENT_RANGE, v);
        }
    }
    if k.cd == 1 {
        let cd = format!("attachment; filename=\"{name}\"");
        if let Ok(v) = HeaderValue::from_str(&cd) {
            h.insert(header::CONTENT_DISPOSITION, v);
        }
    }
    resp
}

/// 416 com o tamanho real.
fn unsatisfiable(size: u64) -> Response {
    let mut resp = StatusCode::RANGE_NOT_SATISFIABLE.into_response();
    if let Ok(v) = HeaderValue::from_str(&format!("bytes */{size}")) {
        resp.headers_mut().insert(header::CONTENT_RANGE, v);
    }
    resp
}

struct BodyState {
    seed: u64,
    pos: u64,
    end: u64,
    rate: Option<u64>,
    drop_after: Option<u64>,
    sent: u64,
    t0: Instant,
    dropped: bool,
    counters: Counters,
    _guard: ConnGuard,
}

/// Gera o corpo em pedaços, respeitando taxa e queda simulada.
fn body_stream(
    state: BodyState,
) -> impl futures_util::Stream<Item = Result<Bytes, std::io::Error>> + Send {
    futures_util::stream::unfold(state, |mut s| async move {
        if s.dropped || s.pos >= s.end {
            return None;
        }
        if s.drop_after.is_some_and(|limit| s.sent >= limit) {
            s.dropped = true;
            return Some((Err(std::io::Error::other("queda simulada")), s));
        }
        let mut chunk: u64 = 64 * 1024;
        if let Some(rate) = s.rate {
            chunk = chunk.min((rate / 20).max(1024));
            let due = Duration::from_secs_f64(s.sent as f64 / rate as f64);
            tokio::time::sleep_until((s.t0 + due).into()).await;
        }
        if let Some(limit) = s.drop_after {
            chunk = chunk.min(limit - s.sent).max(1);
        }
        let n = chunk.min(s.end - s.pos) as usize;
        let mut buf = vec![0u8; n];
        fill(s.seed, s.pos, &mut buf);
        s.pos += n as u64;
        s.sent += n as u64;
        s.counters.add_bytes(n as u64);
        Some((Ok(Bytes::from(buf)), s))
    })
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
