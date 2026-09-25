//! Pixeldrain pela API oficial: `info` dá nome, tamanho, sha256 e se o site
//! está pedindo captcha; o download é `/api/file/{id}?download` (com Range).

mod parse;

use crate::page;
use crate::plugin::{FileInfo, HostCtx, HostPlugin};
use crate::rules::Rules;
use async_trait::async_trait;
use parse::Target;
use swoop_core::{HostError, Integrity, RangeStyle, ResolveRequest, Resolved};
use url::Url;

pub struct Pixeldrain;

impl Pixeldrain {
    /// Endereço da API para `path` (ex.: `file/abc/info`).
    fn api(ctx: &HostCtx, path: &str) -> Result<Url, HostError> {
        let base = ctx.rules.pixeldrain.api.trim_end_matches('/');
        Url::parse(&format!("{base}/{path}"))
            .map_err(|e| HostError::Changed(format!("api do Pixeldrain nas regras: {e}")))
    }

    async fn info(ctx: &HostCtx, id: &str) -> Result<parse::Info, HostError> {
        let res = page::get(ctx, &Self::api(ctx, &format!("file/{id}/info"))?).await?;
        parse::info(res.status, &res.body)
    }

    fn target(ctx: &HostCtx, url: &Url) -> Result<Target, HostError> {
        parse::target(url, &ctx.rules.pixeldrain.hosts).ok_or(HostError::Unsupported)
    }
}

#[async_trait]
impl HostPlugin for Pixeldrain {
    fn id(&self) -> &'static str {
        "pixeldrain"
    }

    fn matches(&self, url: &Url, rules: &Rules) -> bool {
        parse::target(url, &rules.pixeldrain.hosts).is_some()
    }

    async fn check(&self, ctx: &HostCtx, url: &Url) -> Result<FileInfo, HostError> {
        match Self::target(ctx, url)? {
            Target::File(id) => {
                let info = Self::info(ctx, &id).await?;
                Ok(FileInfo {
                    name: Some(info.name),
                    size: Some(info.size),
                })
            }
            Target::List(_) => Ok(FileInfo {
                name: None,
                size: None,
            }),
        }
    }

    async fn resolve(&self, ctx: &HostCtx, req: &ResolveRequest) -> Result<Resolved, HostError> {
        let url = &req.url;
        let Target::File(id) = Self::target(ctx, url)? else {
            return Err(HostError::Changed(
                "é uma lista do Pixeldrain: adicione de novo para abrir os arquivos".into(),
            ));
        };
        let info = Self::info(ctx, &id).await?;
        parse::check_available(&info)?;
        let mut direct = Self::api(ctx, &format!("file/{id}"))?;
        direct.set_query(Some("download"));
        Ok(Resolved {
            url: direct,
            headers: Vec::new(),
            file_name: Some(info.name),
            size: Some(info.size),
            integrity: info
                .hash_sha256
                .filter(|h| h.len() == 64)
                .map(|h| Integrity::Sha256(h.to_ascii_lowercase())),
            range: RangeStyle::Probe,
            max_connections: ctx.rules.pixeldrain.max_connections.max(1),
            resumable: true,
            expires_at: None,
            host_key: self.id().to_owned(),
        })
    }

    async fn expand(&self, ctx: &HostCtx, url: &Url) -> Result<Option<Vec<Url>>, HostError> {
        let Target::List(id) = Self::target(ctx, url)? else {
            return Ok(None);
        };
        let res = page::get(ctx, &Self::api(ctx, &format!("list/{id}"))?).await?;
        let ids = parse::list_files(res.status, &res.body)?;
        let host = url.host_str().unwrap_or("pixeldrain.com");
        ids.iter()
            .map(|f| Url::parse(&format!("https://{host}/u/{f}")))
            .collect::<Result<_, _>>()
            .map(Some)
            .map_err(|e| HostError::Changed(e.to_string()))
    }
}
