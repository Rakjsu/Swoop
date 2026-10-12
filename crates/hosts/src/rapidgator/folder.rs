//! Rapidgator, pastas (parte pura): links de arquivos e de subpastas da
//! página `/folder/<n>/<nome>.html` e a última página (`?page=N`).

use super::RapidgatorRules;
use super::parse::regex;
use swoop_core::HostError;
use url::Url;

/// O que uma página da pasta lista.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FolderPage {
    pub files: Vec<Url>,
    pub folders: Vec<Url>,
    /// Maior número de página citado (1 sem paginação).
    pub last_page: u32,
}

/// Links do padrão (grupo 1), absolutos e sem repetir, na ordem da página.
fn links(html: &str, page_url: &Url, pattern: &str) -> Result<Vec<Url>, HostError> {
    let mut out: Vec<Url> = Vec::new();
    for c in regex(pattern)?.captures_iter(html) {
        if let Some(url) = c.get(1).and_then(|m| page_url.join(m.as_str()).ok())
            && !out.contains(&url)
        {
            out.push(url);
        }
    }
    Ok(out)
}

/// Lê uma página da pasta.
pub fn folder_page(
    status: u16,
    html: &str,
    page_url: &Url,
    rules: &RapidgatorRules,
) -> Result<FolderPage, HostError> {
    let lower = html.to_lowercase();
    if status == 404
        || rules
            .folder_missing_markers
            .iter()
            .any(|m| lower.contains(m.as_str()))
    {
        return Err(HostError::Offline);
    }
    let me = page_url.path();
    let folders = links(html, page_url, &rules.folder_sub_pattern)?
        .into_iter()
        .filter(|u| u.path() != me)
        .collect();
    let last_page = regex(&rules.folder_page_pattern)?
        .captures_iter(html)
        .filter_map(|c| c.get(1)?.as_str().parse::<u32>().ok())
        .max()
        .unwrap_or(1)
        .max(1);
    Ok(FolderPage {
        files: links(html, page_url, &rules.folder_file_pattern)?,
        folders,
        last_page,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rules;

    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/tests/fixtures/rapidgator/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn arquivos_subpastas_e_paginas() {
        let rules = Rules::embedded().rapidgator;
        let page = Url::parse("https://rapidgator.net/folder/4521/Colecao.html").unwrap();
        let f = folder_page(200, &fixture("folder_page1.html"), &page, &rules).unwrap();
        assert_eq!(f.files.len(), 3);
        assert_eq!(
            f.files[0].as_str(),
            "https://rapidgator.net/file/0123456789abcdef0123456789abcdef/parte1.rar.html"
        );
        assert_eq!(f.folders.len(), 1);
        assert_eq!(f.folders[0].path(), "/folder/4522/Extras.html");
        assert_eq!(f.last_page, 2);
        let empty = folder_page(200, &fixture("folder_empty.html"), &page, &rules).unwrap();
        assert_eq!(
            empty,
            FolderPage {
                last_page: 1,
                ..FolderPage::default()
            }
        );
        let gone = "<div class=\"error\">E_FOLDERNOTFOUND</div>";
        assert_eq!(
            folder_page(200, gone, &page, &rules),
            Err(HostError::Offline)
        );
    }
}
