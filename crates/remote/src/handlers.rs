//! Rotas JSON da API. O token já foi conferido pelo `guard::bearer`.

use crate::Shared;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use std::sync::Arc;
use swoop_api::ApiError;
use swoop_api::Command;

/// Histórico padrão quando o cliente não diz quantos.
const HISTORY_DEFAULT: u32 = 200;

/// `ApiError` como resposta HTTP, com o JSON que a UI já entende.
pub struct Failure(ApiError);

impl From<ApiError> for Failure {
    fn from(e: ApiError) -> Self {
        Self(e)
    }
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        let status = match self.0 {
            ApiError::Invalid(_) => StatusCode::BAD_REQUEST,
            ApiError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(self.0)).into_response()
    }
}

/// `POST /api/v1/exec`: um `Command` → `Reply`.
pub async fn exec(
    State(s): State<Arc<Shared>>,
    Json(cmd): Json<Command>,
) -> Result<Response, Failure> {
    Ok(Json(s.backend.exec(cmd).await?).into_response())
}

/// `GET /api/v1/list`: a fila inteira.
pub async fn list(State(s): State<Arc<Shared>>) -> Result<Response, Failure> {
    Ok(Json(s.backend.list().await?).into_response())
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    limit: Option<u32>,
}

/// `GET /api/v1/history?limit=N`: as entradas mais recentes.
pub async fn history(
    State(s): State<Arc<Shared>>,
    Query(q): Query<HistoryQuery>,
) -> Result<Response, Failure> {
    let limit = q.limit.unwrap_or(HISTORY_DEFAULT);
    Ok(Json(s.backend.history(limit).await?).into_response())
}

/// `GET /api/v1/collector`: links do coletor.
pub async fn collector(State(s): State<Arc<Shared>>) -> Result<Response, Failure> {
    Ok(Json(s.backend.collector().await?).into_response())
}

/// `GET /api/v1/settings`: preferências em uso.
pub async fn settings(State(s): State<Arc<Shared>>) -> Result<Response, Failure> {
    Ok(Json(s.backend.settings().await?).into_response())
}
