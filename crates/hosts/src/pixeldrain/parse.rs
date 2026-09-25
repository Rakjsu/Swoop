//! Pixeldrain, parte pura: formatos de link e respostas da API oficial
//! (`/api/file/{id}/info`, `/api/list/{id}`).

use serde::Deserialize;
use swoop_core::HostError;
use url::Url;

/// O que o link aponta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    File(String),
    List(String),
}

/// Formatos aceitos: `/u/{id}`, `/api/file/{id}[/info]`, `/l/{id}`,
/// `/api/list/{id}` (com ou sem `www.`, `?download`, barra final).
pub fn target(url: &Url, hosts: &[String]) -> Option<Target> {
    if !crate::Rules::host_in(hosts, url.host_str()?) {
        return None;
    }
    let segs: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    let (kind, id) = match segs.as_slice() {
        ["u", id] | ["api", "file", id] | ["api", "file", id, "info"] => ("file", *id),
        ["l", id] | ["api", "list", id] => ("list", *id),
        _ => return None,
    };
    let valid = id.len() >= 4 && id.chars().all(|c| c.is_ascii_alphanumeric());
    match (valid, kind) {
        (true, "file") => Some(Target::File(id.to_owned())),
        (true, _) => Some(Target::List(id.to_owned())),
        _ => None,
    }
}

/// Dados de `/api/file/{id}/info` que interessam.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Info {
    pub name: String,
    pub size: u64,
    #[serde(default)]
    pub hash_sha256: Option<String>,
    /// Vazio = liberado; `*_captcha_required` = o site quer captcha.
    #[serde(default)]
    pub availability: String,
    #[serde(default)]
    pub availability_message: String,
    /// Preenchido quando o arquivo foi bloqueado por denúncia.
    #[serde(default)]
    pub abuse_type: String,
}

/// Erro da API (`{"success":false,"value":"not_found",…}`).
#[derive(Deserialize)]
struct ApiError {
    value: String,
    #[serde(default)]
    message: String,
}

/// Lê a resposta de `info` (status HTTP + corpo).
pub fn info(status: u16, body: &str) -> Result<Info, HostError> {
    if status == 200 {
        let info: Info = serde_json::from_str(body)
            .map_err(|e| HostError::Changed(format!("resposta de info do Pixeldrain: {e}")))?;
        if !info.abuse_type.is_empty() {
            return Err(HostError::Offline);
        }
        return Ok(info);
    }
    Err(api_error(status, body))
}

/// O arquivo pode ser baixado agora? Captcha nunca é contornado.
pub fn check_available(info: &Info) -> Result<(), HostError> {
    if info.availability.is_empty() {
        return Ok(());
    }
    let why = if info.availability_message.is_empty() {
        info.availability.replace('_', " ")
    } else {
        info.availability_message.clone()
    };
    Err(HostError::BrowserRequired(format!(
        "o Pixeldrain pediu captcha ({why}); abra o link no navegador ou tente mais tarde"
    )))
}

/// Arquivos de uma lista (`/api/list/{id}`), na ordem.
pub fn list_files(status: u16, body: &str) -> Result<Vec<String>, HostError> {
    #[derive(Deserialize)]
    struct List {
        files: Vec<Item>,
    }
    #[derive(Deserialize)]
    struct Item {
        id: String,
    }
    if status != 200 {
        return Err(api_error(status, body));
    }
    let list: List = serde_json::from_str(body)
        .map_err(|e| HostError::Changed(format!("resposta de lista do Pixeldrain: {e}")))?;
    Ok(list.files.into_iter().map(|f| f.id).collect())
}

fn api_error(status: u16, body: &str) -> HostError {
    let err = serde_json::from_str::<ApiError>(body).ok();
    match (status, err.as_ref().map(|e| e.value.as_str())) {
        (404, _) | (_, Some("not_found")) => HostError::Offline,
        (403, Some(v)) if v.contains("captcha") => {
            let why = err.map(|e| e.message).unwrap_or_default();
            HostError::BrowserRequired(format!(
                "o Pixeldrain pediu captcha ({why}); abra o link no navegador ou tente mais tarde"
            ))
        }
        _ => HostError::Http(status),
    }
}

#[cfg(test)]
#[path = "parse_tests.rs"]
mod tests;
