//! Mediafire: API pública 1.5 para nome/tamanho/sha256 e pastas; a página do
//! arquivo dá o link de download (que expira: 403 → resolver de novo).

mod parse;

use crate::page;
use crate::plugin::{FileInfo, HostCtx, HostPlugin, MAX_FOLDER_DEPTH};
use crate::rules::Rules;
use async_trait::async_trait;
use parse::Target;
use swoop_core::{HostError, Integrity, RangeStyle, ResolveRequest, Resolved};
use url::Url;

/// Pedaços lidos no máximo por pasta (100 itens cada).
const MAX_CHUNKS: u32 = 50;

pub struct Mediafire;

impl Mediafire {
    fn target(ctx: &HostCtx, url: &Url) -> Result<Target, HostError> {
        parse::target(url, &ctx.rules.mediafire.hosts).ok_or(HostError::Unsupported)
    }

    /// Endereço da API com os parâmetros dados.
    fn api(ctx: &HostCtx, path: &str, query: &[(&str, &str)]) -> Result<Url, HostError> {
        let base = ctx.rules.mediafire.api.trim_end_matches('/');
        let mut url = Url::parse(&format!("{base}/{path}"))
            .map_err(|e| HostError::Changed(format!("api do Mediafire nas regras: {e}")))?;
        url.query_pairs_mut()
            .extend_pairs(query)
            .append_pair("response_format", "json");
        Ok(url)
    }

    async fn meta(ctx: &HostCtx, key: &str) -> Result<parse::FileMeta, HostError> {
        let url = Self::api(ctx, "file/get_info.php", &[("quick_key", key)])?;
        parse::file_meta(&page::get(ctx, &url).await?.body)
    }

    fn file_url(key: &str) -> Result<Url, HostError> {
        Url::parse(&format!("https://www.mediafire.com/file/{key}"))
            .map_err(|e| HostError::Changed(e.to_string()))
    }

    /// Arquivos da pasta `key`, abrindo subpastas até `MAX_FOLDER_DEPTH`.
    async fn folder_files(
        ctx: &HostCtx,
        key: &str,
        depth: u32,
        out: &mut Vec<Url>,
    ) -> Result<(), HostError> {
        for kind in ["files", "folders"] {
            for chunk in 1..=MAX_CHUNKS {
                let n = chunk.to_string();
                let query = [
                    ("folder_key", key),
                    ("content_type", kind),
                    ("chunk", n.as_str()),
                ];
                let res =
                    page::get(ctx, &Self::api(ctx, "folder/get_content.php", &query)?).await?;
                let c = parse::folder_chunk(&res.body)?;
                for k in &c.files {
                    out.push(Self::file_url(k)?);
                }
                for sub in c.folders.iter().filter(|_| depth < MAX_FOLDER_DEPTH) {
                    Box::pin(Self::folder_files(ctx, sub, depth + 1, out)).await?;
                }
                if !c.more {
                    break;
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl HostPlugin for Mediafire {
    fn id(&self) -> &'static str {
        "mediafire"
    }

    fn matches(&self, url: &Url, rules: &Rules) -> bool {
        parse::target(url, &rules.mediafire.hosts).is_some()
    }

    async fn check(&self, ctx: &HostCtx, url: &Url) -> Result<FileInfo, HostError> {
        match Self::target(ctx, url)? {
            Target::File(key) => {
                let meta = Self::meta(ctx, &key).await?;
                Ok(FileInfo {
                    name: Some(meta.name),
                    size: meta.size,
                })
            }
            Target::Folder(_) => Ok(FileInfo {
                name: None,
                size: None,
            }),
        }
    }

    async fn resolve(&self, ctx: &HostCtx, req: &ResolveRequest) -> Result<Resolved, HostError> {
        let url = &req.url;
        let Target::File(key) = Self::target(ctx, url)? else {
            return Err(HostError::Changed(
                "é uma pasta do Mediafire: adicione de novo para abrir os arquivos".into(),
            ));
        };
        let meta = Self::meta(ctx, &key).await?;
        let res = page::get(ctx, &Self::file_url(&key)?).await?;
        // Alguns links redirecionam direto para o arquivo.
        let dl = if res.file {
            parse::Download {
                url: res.url.clone(),
                name: None,
                size: None,
            }
        } else {
            parse::download_page(&res.url, &res.body, &ctx.rules.mediafire)?
        };
        Ok(Resolved {
            url: dl.url,
            headers: vec![("Referer".into(), res.url.to_string())],
            file_name: Some(meta.name).or(dl.name),
            size: meta.size.or(dl.size),
            integrity: meta.sha256.map(Integrity::Sha256),
            range: RangeStyle::Probe,
            max_connections: ctx.rules.mediafire.max_connections.max(1),
            resumable: true,
            expires_at: None,
            host_key: self.id().to_owned(),
        })
    }

    async fn expand(&self, ctx: &HostCtx, url: &Url) -> Result<Option<Vec<Url>>, HostError> {
        let Target::Folder(key) = Self::target(ctx, url)? else {
            return Ok(None);
        };
        let mut files = Vec::new();
        Self::folder_files(ctx, &key, 1, &mut files).await?;
        Ok(Some(files))
    }
}
