//! Download do instalador com conferência do sha256 durante a gravação.

use crate::release::parse_sums;
use crate::{Update, UpdateError};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

/// Tamanho máximo aceito para o `SHA256SUMS.txt`.
const MAX_SUMS: usize = 64 * 1024;

/// Baixa o instalador para `dir`, conferindo o hash publicado. Chama
/// `progress(recebido, total)` a cada pedaço. Devolve o caminho final.
pub async fn download(
    client: &swoop_net::reqwest::Client,
    update: &Update,
    dir: &Path,
    progress: impl Fn(u64, Option<u64>),
) -> Result<PathBuf, UpdateError> {
    let sums = get_text(client, &update.sums.url).await?;
    let expected = parse_sums(&sums, &update.setup.name)
        .ok_or_else(|| UpdateError::Missing(format!("o sha256 de {}", update.setup.name)))?;
    fetch_verified(
        client,
        &update.setup.url,
        &update.setup.name,
        &expected,
        dir,
        progress,
    )
    .await
}

/// Baixa `url` para `dir/name` só se o sha256 bater (grava em `.part` antes).
pub async fn fetch_verified(
    client: &swoop_net::reqwest::Client,
    url: &str,
    name: &str,
    expected_sha256: &str,
    dir: &Path,
    progress: impl Fn(u64, Option<u64>),
) -> Result<PathBuf, UpdateError> {
    let io = |e: std::io::Error| UpdateError::Io(e.to_string());
    tokio::fs::create_dir_all(dir).await.map_err(io)?;
    let part = dir.join(format!("{name}.part"));
    let target = dir.join(name);

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(UpdateError::Http(resp.status().as_u16()));
    }
    let total = resp.content_length();
    let mut file = tokio::fs::File::create(&part).await.map_err(io)?;
    let mut hasher = Sha256::new();
    let mut received = 0u64;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| UpdateError::Network(e.to_string()))?;
        hasher.update(&chunk);
        file.write_all(&chunk).await.map_err(io)?;
        received += chunk.len() as u64;
        progress(received, total);
    }
    file.flush().await.map_err(io)?;
    drop(file);

    if hex::encode(hasher.finalize()) != expected_sha256.to_ascii_lowercase() {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(UpdateError::HashMismatch);
    }
    let _ = tokio::fs::remove_file(&target).await;
    tokio::fs::rename(&part, &target).await.map_err(io)?;
    Ok(target)
}

/// GET de um arquivo de texto pequeno.
async fn get_text(client: &swoop_net::reqwest::Client, url: &str) -> Result<String, UpdateError> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(UpdateError::Http(resp.status().as_u16()));
    }
    let body = resp
        .bytes()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    if body.len() > MAX_SUMS {
        return Err(UpdateError::BadResponse(
            "lista de hashes grande demais".into(),
        ));
    }
    String::from_utf8(body.to_vec()).map_err(|e| UpdateError::BadResponse(e.to_string()))
}
