//! Links colados → endereços para a fila: valida e abre pastas/listas dos
//! servidores (Drive, Mediafire, Pixeldrain) nos arquivos que elas têm.

use crate::ServiceError;
use swoop_hosts::Registry;
use swoop_net::Redacted;
use url::Url;

/// Valida os links e troca cada pasta pelos seus arquivos. Pasta que não
/// abre (removida, rede) fica como está: o download mostra o motivo.
pub async fn prepare(registry: &Registry, links: &[String]) -> Result<Vec<String>, ServiceError> {
    let urls: Vec<Url> = links
        .iter()
        .map(|l| Url::parse(l.trim()).map_err(|_| ServiceError::BadLink(l.clone())))
        .collect::<Result<_, _>>()?;
    let mut out = Vec::with_capacity(urls.len());
    for url in urls {
        match registry.expand(&url).await {
            Ok(Some(files)) => out.extend(files.into_iter().map(String::from)),
            Ok(None) => out.push(url.into()),
            Err(e) => {
                tracing::warn!("não deu para abrir a pasta {}: {e}", Redacted(&url));
                out.push(url.into());
            }
        }
    }
    if out.is_empty() {
        return Err(ServiceError::BadLink("nenhum arquivo nos links".into()));
    }
    Ok(out)
}
