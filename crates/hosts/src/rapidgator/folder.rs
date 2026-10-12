//! Rapidgator, pastas: links de arquivos e de subpastas da página
//! `/folder/<n>/<nome>.html` e a última página (`?page=N`) — parte pura — e a
//! leitura de todas as páginas e subpastas.

use super::parse::{blocked, regex};
use super::{HOST, RapidgatorRules, free};
use crate::page;
use crate::plugin::{HostCtx, MAX_FOLDER_DEPTH};
use std::collections::HashSet;
use std::time::{Duration, SystemTime};
use swoop_core::HostError;
use url::Url;

/// Link da página `n` da pasta (o `page` que veio no link colado sai).
pub fn page_url(dir: &Url, n: u32) -> Url {
    let keep: Vec<(String, String)> = dir
        .query_pairs()
        .filter(|(k, _)| k != "page")
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let mut url = dir.clone();
    url.set_query(None);
    if !keep.is_empty() || n > 1 {
        let mut q = url.query_pairs_mut();
        q.extend_pairs(keep);
        if n > 1 {
            q.append_pair("page", &n.to_string());
        }
    }
    url
}

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
    // Cloudflare, limite, fora do ar: o motivo de sempre; outro erro HTTP
    // nunca vira pasta vazia.
    blocked(status, html, rules, SystemTime::now())?;
    if status >= 400 {
        return Err(HostError::Http(status));
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

/// Arquivos da pasta e das subpastas (até `MAX_FOLDER_DEPTH` níveis),
/// página a página, com uma pausa entre as páginas.
pub async fn expand(ctx: &HostCtx, root: Url) -> Result<Vec<Url>, HostError> {
    let rules = &ctx.rules.rapidgator;
    let origin = free::origin(ctx)?;
    let (mut files, mut seen) = (Vec::new(), HashSet::new());
    let mut todo = vec![(root.clone(), 0)];
    while let Some((dir, depth)) = todo.pop() {
        if !seen.insert(dir.path().to_owned()) {
            continue;
        }
        for n in 1..=rules.folder_max_pages {
            if n > 1 {
                tokio::time::sleep(Duration::from_millis(rules.folder_page_delay_ms)).await;
            }
            let url = page_url(&dir, n);
            let page = page::get_from(ctx, &url, &origin, false, None).await?;
            let listed = match folder_page(page.status, &page.body, &page.url, rules) {
                Ok(listed) => listed,
                // subpasta que sumiu não derruba a pasta inteira
                Err(HostError::Offline) if dir != root => break,
                Err(e) => return Err(ctx.changed(HOST, "pasta", &page.body, e)),
            };
            for f in listed.files {
                if !files.contains(&f) {
                    files.push(f);
                }
            }
            if depth < MAX_FOLDER_DEPTH {
                todo.extend(listed.folders.into_iter().map(|u| (u, depth + 1)));
            }
            if n >= listed.last_page {
                break;
            }
        }
    }
    Ok(files)
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
        // erro do site nunca vira pasta vazia
        let err = folder_page(500, "<html>erro</html>", &page, &rules);
        assert_eq!(err, Err(HostError::Http(500)));
        let cf = folder_page(503, &fixture("cloudflare.html"), &page, &rules);
        assert!(matches!(cf, Err(HostError::BrowserRequired(_))));
    }

    #[test]
    fn pagina_n_troca_o_page_do_link() {
        let dir = Url::parse("https://rapidgator.net/folder/4521/Colecao.html?page=3&x=1").unwrap();
        let base = "https://rapidgator.net/folder/4521/Colecao.html";
        assert_eq!(page_url(&dir, 1).as_str(), format!("{base}?x=1"));
        assert_eq!(page_url(&dir, 2).as_str(), format!("{base}?x=1&page=2"));
        let plain = Url::parse(base).unwrap();
        assert_eq!(page_url(&plain, 1), plain);
    }
}
