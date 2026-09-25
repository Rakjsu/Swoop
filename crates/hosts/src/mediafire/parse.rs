//! Mediafire, parte pura: formatos de link, API `get_info`/`get_content` e a
//! página do arquivo (botão de download, captcha, aviso de arquivo perigoso).

use crate::page::{self, human_size};
use crate::rules::MediafireRules;
use base64::Engine;
use scraper::Html;
use serde::Deserialize;
use swoop_core::HostError;
use url::Url;

/// O que o link aponta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// `quick_key` do arquivo.
    File(String),
    /// `folder_key` da pasta.
    Folder(String),
}

/// Formatos: `/file/{k}[/nome][/file]`, `/file_premium/…`, `/download/…`,
/// `/view/…`, `/folder/{k}[/nome]` e o antigo `/?{k}`.
pub fn target(url: &Url, hosts: &[String]) -> Option<Target> {
    if !crate::Rules::host_in(hosts, url.host_str()?) {
        return None;
    }
    let segs: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    let (folder, key) = match segs.as_slice() {
        ["file" | "file_premium" | "download" | "view", key, ..] => (false, *key),
        ["folder", key, ..] => (true, *key),
        [] => (false, url.query().filter(|q| !q.contains('='))?),
        _ => return None,
    };
    let valid = key.len() >= 6 && key.chars().all(|c| c.is_ascii_alphanumeric());
    match (valid, folder) {
        (true, false) => Some(Target::File(key.to_ascii_lowercase())),
        (true, true) => Some(Target::Folder(key.to_owned())),
        _ => None,
    }
}

/// Nome, tamanho e sha256 (API `file/get_info`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMeta {
    pub name: String,
    pub size: Option<u64>,
    pub sha256: Option<String>,
}

#[derive(Deserialize)]
struct Envelope<T> {
    response: T,
}

#[derive(Deserialize)]
struct InfoResponse {
    result: String,
    file_info: Option<RawFile>,
}

#[derive(Deserialize)]
struct RawFile {
    #[serde(default)]
    quickkey: String,
    #[serde(default)]
    filename: String,
    #[serde(default)]
    size: String,
    #[serde(default)]
    hash: String,
}

/// Lê `get_info`; `result: Error` (chave inválida/removida) = offline.
pub fn file_meta(body: &str) -> Result<FileMeta, HostError> {
    let env: Envelope<InfoResponse> = serde_json::from_str(body)
        .map_err(|e| HostError::Changed(format!("get_info do Mediafire: {e}")))?;
    let info = match (env.response.result.as_str(), env.response.file_info) {
        ("Success", Some(f)) => f,
        _ => return Err(HostError::Offline),
    };
    Ok(FileMeta {
        name: info.filename,
        size: info.size.parse().ok(),
        sha256: Some(info.hash.to_ascii_lowercase()).filter(|h| h.len() == 64),
    })
}

/// Um pedaço de `folder/get_content` (`content_type` = files ou folders).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// `quickkey` dos arquivos.
    pub files: Vec<String>,
    /// `folderkey` das subpastas.
    pub folders: Vec<String>,
    /// Há mais pedaços depois deste.
    pub more: bool,
}

/// Lê um pedaço de `folder/get_content`; pasta inexistente = offline.
pub fn folder_chunk(body: &str) -> Result<Chunk, HostError> {
    #[derive(Deserialize)]
    struct Resp {
        result: String,
        folder_content: Option<Content>,
    }
    #[derive(Deserialize)]
    struct Content {
        #[serde(default)]
        files: Vec<RawFile>,
        #[serde(default)]
        folders: Vec<RawFolder>,
        #[serde(default)]
        more_chunks: String,
    }
    #[derive(Deserialize)]
    struct RawFolder {
        folderkey: String,
    }
    let env: Envelope<Resp> = serde_json::from_str(body)
        .map_err(|e| HostError::Changed(format!("get_content do Mediafire: {e}")))?;
    match (env.response.result.as_str(), env.response.folder_content) {
        ("Success", Some(c)) => Ok(Chunk {
            files: c.files.into_iter().map(|f| f.quickkey).collect(),
            folders: c.folders.into_iter().map(|f| f.folderkey).collect(),
            more: c.more_chunks == "yes",
        }),
        _ => Err(HostError::Offline),
    }
}

/// O que a página do arquivo oferece.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    pub url: Url,
    pub name: Option<String>,
    pub size: Option<u64>,
}

/// Lê a página do arquivo (endereço final + HTML) pelas regras.
pub fn download_page(
    final_url: &Url,
    html: &str,
    rules: &MediafireRules,
) -> Result<Download, HostError> {
    if final_url.path().contains("error.php") {
        return Err(HostError::Offline);
    }
    let doc = Html::parse_document(html);
    let href = page::attr(&doc, &rules.download_selector, "href")?;
    let scrambled = page::attr(&doc, &rules.download_selector, &rules.scrambled_attr)?;
    let link = scrambled
        .and_then(|s| {
            base64::engine::general_purpose::STANDARD
                .decode(s.trim())
                .ok()
        })
        .and_then(|b| String::from_utf8(b).ok())
        .or(href)
        .and_then(|l| Url::parse(&l).ok())
        .filter(|u| matches!(u.scheme(), "http" | "https"));
    let Some(url) = link else {
        return Err(blocked_reason(html, rules));
    };
    let name = page::attr(&doc, &rules.name_selector, "title")?;
    let button = page::text(&doc, &rules.download_selector)?.unwrap_or_default();
    let size = button
        .rsplit_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
        .and_then(|(s, _)| human_size(s));
    Ok(Download { url, name, size })
}

/// Sem botão de download: senha, arquivo perigoso, captcha ou página
/// desconhecida.
fn blocked_reason(html: &str, rules: &MediafireRules) -> HostError {
    let lower = html.to_lowercase();
    let has = |markers: &[String]| markers.iter().any(|m| lower.contains(m.as_str()));
    if has(&rules.password_markers) {
        HostError::BrowserRequired(
            "o arquivo do Mediafire tem senha; abra o link no navegador".into(),
        )
    } else if has(&rules.danger_markers) {
        HostError::BrowserRequired(
            "o Mediafire marcou o arquivo como possivelmente perigoso; confira no navegador".into(),
        )
    } else if has(&rules.captcha_markers) {
        HostError::BrowserRequired("o Mediafire pediu captcha; abra o link no navegador".into())
    } else {
        HostError::Changed("página do Mediafire sem o botão de download".into())
    }
}

#[cfg(test)]
#[path = "parse_tests.rs"]
mod tests;
