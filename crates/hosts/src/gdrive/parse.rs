//! Google Drive, parte pura: formatos de link, a resposta de
//! `drive.usercontent.google.com/download` (arquivo, confirmação de
//! "não dá para verificar vírus", cota, removido, privado) e a pasta.

use crate::page::{self, human_size};
use crate::rules::GdriveRules;
use scraper::Html;
use std::time::{Duration, SystemTime};
use swoop_core::{HostError, WaitReason};
use url::Url;

/// O que o link aponta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    File(String),
    Folder(String),
}

/// Formatos: `/file/d/{id}/…`, `/file/u/{n}/d/{id}`, `/open?id=`, `/uc?id=`,
/// `/download?id=` (usercontent), `/drive/[u/{n}/]folders/{id}`,
/// `/folderview?id=`, em drive/docs/drive.usercontent.
pub fn target(url: &Url, hosts: &[String]) -> Option<Target> {
    if !crate::Rules::host_in(hosts, url.host_str()?) {
        return None;
    }
    let query_id = || {
        url.query_pairs()
            .find(|(k, _)| k == "id")
            .map(|(_, v)| v.into_owned())
    };
    let segs: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    let (folder, id) = match segs.as_slice() {
        ["file", "d", id, ..] | ["file", "u", _, "d", id, ..] => (false, (*id).to_owned()),
        ["open"] | ["uc"] | ["download"] => (false, query_id()?),
        ["drive", "folders", id, ..] | ["drive", "u", _, "folders", id, ..] => {
            (true, (*id).to_owned())
        }
        ["folderview"] | ["embeddedfolderview"] => (true, query_id()?),
        _ => return None,
    };
    let valid = id.len() >= 20
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    match (valid, folder) {
        (true, false) => Some(Target::File(id)),
        (true, true) => Some(Target::Folder(id)),
        _ => None,
    }
}

/// Resposta do download (cabeçalhos que importam + corpo, se for HTML).
pub struct Response<'a> {
    pub status: u16,
    pub final_url: &'a Url,
    /// Veio o próprio arquivo (anexo), não uma página.
    pub file: bool,
    /// Nome do `Content-Disposition`.
    pub name: Option<String>,
    /// Tamanho total (Content-Range/Content-Length), quando é o arquivo.
    pub size: Option<u64>,
    pub body: &'a str,
}

/// O que fazer com a resposta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Veio o arquivo: o mesmo endereço serve para baixar.
    File {
        name: Option<String>,
        size: Option<u64>,
    },
    /// Página de confirmação: baixar pelo endereço do formulário.
    Confirm {
        url: Url,
        name: Option<String>,
        size: Option<u64>,
    },
}

/// Classifica a resposta pelas regras.
pub fn outcome(
    r: &Response<'_>,
    rules: &GdriveRules,
    now: SystemTime,
) -> Result<Outcome, HostError> {
    if r.final_url.host_str() == Some("accounts.google.com") {
        return Err(HostError::AccessDenied);
    }
    if r.file {
        return Ok(Outcome::File {
            name: r.name.clone(),
            size: r.size,
        });
    }
    let lower = r.body.to_lowercase();
    let has = |markers: &[String]| markers.iter().any(|m| lower.contains(m.as_str()));
    if has(&rules.quota_markers) {
        return Err(HostError::Wait {
            until: now + Duration::from_secs(rules.quota_wait_minutes * 60),
            reason: WaitReason::Quota,
        });
    }
    if r.status == 404 || has(&rules.missing_markers) {
        return Err(HostError::Offline);
    }
    if matches!(r.status, 401 | 403) {
        return Err(HostError::AccessDenied);
    }
    let doc = Html::parse_document(r.body);
    if let Some(url) = confirm_url(&doc, rules)? {
        let name = page::text(&doc, &rules.confirm_name_selector)?;
        let size = page::text(&doc, &rules.confirm_size_selector)?.and_then(|t| {
            t.rsplit_once('(')
                .and_then(|(_, rest)| rest.split_once(')'))
                .and_then(|(s, _)| human_size(s))
        });
        return Ok(Outcome::Confirm { url, name, size });
    }
    Err(HostError::Changed("resposta do Drive desconhecida".into()))
}

/// Endereço do formulário de confirmação com os campos ocultos.
fn confirm_url(doc: &Html, rules: &GdriveRules) -> Result<Option<Url>, HostError> {
    let form_sel = page::selector(&rules.confirm_form_selector)?;
    let input_sel = page::selector("input[type=hidden]")?;
    let Some(form) = doc.select(&form_sel).next() else {
        return Ok(None);
    };
    let Some(mut url) = form.value().attr("action").and_then(|a| Url::parse(a).ok()) else {
        return Ok(None);
    };
    {
        let mut q = url.query_pairs_mut();
        for input in form.select(&input_sel) {
            let (Some(name), Some(value)) =
                (input.value().attr("name"), input.value().attr("value"))
            else {
                continue;
            };
            q.append_pair(name, value);
        }
    }
    Ok(Some(url))
}

/// Itens de uma pasta (`embeddedfolderview`): arquivos e subpastas.
pub fn folder_entries(html: &str, rules: &GdriveRules) -> Result<Vec<Target>, HostError> {
    let doc = Html::parse_document(html);
    let sel = page::selector(&rules.folder_entry_selector)?;
    let mut out = Vec::new();
    for a in doc.select(&sel) {
        let Some(href) = a.value().attr("href") else {
            continue;
        };
        let Ok(url) = Url::parse(href) else { continue };
        if let Some(t) = target(&url, &rules.hosts)
            && !out.contains(&t)
        {
            out.push(t);
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "parse_tests.rs"]
mod tests;
