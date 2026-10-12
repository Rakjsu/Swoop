//! Pastas do Rapidgator falso: `/folder/{n}/{nome}` com `files` arquivos
//! (padrão 2), `per_page` por página (padrão 50, `?page=N`), `sub=1` (uma
//! subpasta `/folder/{n+1}/Sub.html` com 2 arquivos) e `missing=1`.

use crate::file::Shared;
use crate::rapidgator_pages as pages;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct FolderKnobs {
    pub files: u64,
    pub per_page: u64,
    pub sub: u8,
    pub missing: u8,
    pub page: u64,
}

impl Default for FolderKnobs {
    fn default() -> Self {
        Self {
            files: 2,
            per_page: 50,
            sub: 0,
            missing: 0,
            page: 1,
        }
    }
}

/// Id (32 hex) do arquivo `i` da pasta `n`.
pub fn file_id(folder: u64, i: u64) -> String {
    format!("{:032x}", folder * 1000 + i)
}

/// Página da pasta.
pub async fn folder(
    State(st): State<Shared>,
    Path((n, name)): Path<(u64, String)>,
    Query(k): Query<FolderKnobs>,
) -> Response {
    st.rg.0.lock().unwrap().stats.folder_pages += 1;
    if k.missing == 1 {
        return pages::html(
            StatusCode::OK,
            "<div class=\"error\">E_FOLDERNOTFOUND</div>".into(),
        );
    }
    let per = k.per_page.max(1);
    let first = (k.page.max(1) - 1) * per;
    let rows: String = (first..(first + per).min(k.files))
        .map(|i| {
            format!(
                "<tr><td><a href=\"/file/{}/arquivo{i}.bin.html\">arquivo{i}.bin</a></td></tr>",
                file_id(n, i)
            )
        })
        .collect();
    let sub = if k.sub == 1 {
        format!(
            "<tr><td><a href=\"/folder/{}/Sub.html\">Sub</a></td></tr>",
            n + 1
        )
    } else {
        String::new()
    };
    let pager: String = (1..=k.files.div_ceil(per))
        .filter(|_| k.files > per)
        .map(|p| format!("<a href=\"/folder/{n}/{name}?page={p}\">{p}</a> "))
        .collect();
    pages::html(
        StatusCode::OK,
        format!(
            "<html><head><title>Download file {name}</title></head><body>\
             <table>{sub}{rows}</table><div class=\"pager\">{pager}</div></body></html>"
        ),
    )
}
