//! Links colados → fila: valida, abre pastas/listas dos servidores (Drive,
//! Mediafire, Pixeldrain) nos arquivos que elas têm e cria o pacote. Usado
//! pelo `add_links` direto e pelo "Iniciar" do coletor.

use crate::{ServiceError, paths::default_download_dir};
use std::path::Path;
use swoop_core::DownloadId;
use swoop_engine::Engine;
use swoop_hosts::Registry;
use swoop_net::Redacted;
use swoop_store::{Store, downloads, packages};
use url::Url;

/// Valida os links e abre as pastas; nenhum arquivo no fim = erro.
pub async fn prepare(registry: &Registry, links: &[String]) -> Result<Vec<Url>, ServiceError> {
    let urls: Vec<Url> = links
        .iter()
        .map(|l| Url::parse(l.trim()).map_err(|_| ServiceError::BadLink(l.clone())))
        .collect::<Result<_, _>>()?;
    let out = expand_all(registry, urls).await;
    if out.is_empty() {
        return Err(ServiceError::BadLink("nenhum arquivo nos links".into()));
    }
    Ok(out)
}

/// Troca cada pasta pelos seus arquivos. Pasta que não abre (removida, rede)
/// fica como está: o download (ou o coletor) mostra o motivo.
pub async fn expand_all(registry: &Registry, urls: Vec<Url>) -> Vec<Url> {
    let mut out = Vec::with_capacity(urls.len());
    for url in urls {
        match registry.expand(&url).await {
            Ok(Some(files)) => out.extend(files),
            Ok(None) => out.push(url),
            Err(e) => {
                tracing::warn!("não deu para abrir a pasta {}: {e}", Redacted(&url));
                out.push(url);
            }
        }
    }
    out
}

/// Cria um pacote com os links e acorda o motor. Com `dest`, tudo vai para
/// lá; sem, a pasta é automática por tipo (vídeos, músicas, downloads).
pub async fn enqueue(
    store: &Store,
    engine: &Engine,
    urls: Vec<String>,
    dest: Option<&Path>,
    package_name: &str,
) -> Result<Vec<DownloadId>, ServiceError> {
    let auto = dest.is_none();
    let dest = dest
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_download_dir(&engine.settings()));
    let name = package_name.to_owned();
    let ids = store
        .call(move |c| {
            let pkg = packages::insert(c, &name, &dest, auto)?;
            urls.iter().map(|u| downloads::insert(c, pkg, u)).collect()
        })
        .await?;
    engine.wake();
    Ok(ids)
}
