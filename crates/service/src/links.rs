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

/// Links com a chave do servidor (`Registry::host_key`), prontos para a fila.
pub fn keyed(registry: &Registry, urls: &[Url]) -> Vec<(String, String)> {
    urls.iter()
        .map(|u| (u.to_string(), registry.host_key(u)))
        .collect()
}

/// Ao abrir: a chave do servidor das linhas que ainda vão andar passa a ser a
/// do plugin atual (linhas de versões antigas tinham o domínio ou nada, e
/// escapavam das regras do servidor, como o "um por vez" do Rapidgator).
pub async fn rekey(store: &Store, registry: &Registry) -> Result<(), ServiceError> {
    let rows = store.call(|c| downloads::list(c)).await?;
    let changed: Vec<(DownloadId, String)> = rows
        .iter()
        .filter(|r| !r.state.is_final())
        .filter_map(|r| {
            let key = registry.host_key(&Url::parse(&r.url).ok()?);
            (r.host_key.as_deref() != Some(key.as_str())).then_some((r.id, key))
        })
        .collect();
    if !changed.is_empty() {
        store
            .call(move |c| {
                changed
                    .iter()
                    .try_for_each(|(id, key)| downloads::set_host_key(c, *id, key))
            })
            .await?;
    }
    Ok(())
}

/// Cria um pacote com os links (e a chave do servidor de cada um) e acorda o
/// motor. Com `dest`, tudo vai para lá; sem, a pasta é automática por tipo
/// (vídeos, músicas, downloads).
pub async fn enqueue(
    store: &Store,
    engine: &Engine,
    links: Vec<(String, String)>,
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
            links
                .iter()
                .map(|(url, host)| downloads::insert(c, pkg, url, Some(host)))
                .collect()
        })
        .await?;
    engine.wake();
    Ok(ids)
}
