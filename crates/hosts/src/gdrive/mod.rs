//! Google Drive: `drive.usercontent.google.com/download` entrega o arquivo
//! direto (pequeno) ou uma página de confirmação (grande, sem verificação de
//! vírus), cujo formulário é o link final. Cota ("muitos usuários") vira
//! espera; privado vira "exige login". Pastas pela página embutível.

mod parse;

use crate::page::{self, network};
use crate::plugin::{FileInfo, HostCtx, HostPlugin, MAX_FOLDER_DEPTH};
use crate::rules::Rules;
use async_trait::async_trait;
use parse::{Outcome, Target};
use std::time::SystemTime;
use swoop_core::{HostError, RangeStyle, Resolved};
use swoop_net::reqwest::header;
use swoop_net::{parse_content_disposition, parse_content_range};
use url::Url;

pub struct Gdrive;

impl Gdrive {
    fn target(ctx: &HostCtx, url: &Url) -> Result<Target, HostError> {
        parse::target(url, &ctx.rules.gdrive.hosts).ok_or(HostError::Unsupported)
    }

    fn download_url(ctx: &HostCtx, id: &str) -> Result<Url, HostError> {
        let mut url = Url::parse(&ctx.rules.gdrive.download_url)
            .map_err(|e| HostError::Changed(format!("download do Drive nas regras: {e}")))?;
        url.query_pairs_mut()
            .append_pair("id", id)
            .append_pair("export", "download");
        Ok(url)
    }

    /// Pede o 1º byte: arquivo (cabeçalhos) ou página (corpo) → `Outcome`.
    async fn probe(ctx: &HostCtx, id: &str) -> Result<(Url, Outcome), HostError> {
        let url = Self::download_url(ctx, id)?;
        let res = ctx
            .http
            .get(url.clone())
            .header(header::RANGE, "bytes=0-0")
            .header(header::ACCEPT_ENCODING, "identity")
            .send()
            .await
            .map_err(network)?;
        let status = res.status().as_u16();
        let final_url = res.url().clone();
        let h = res.headers();
        let text = |name: header::HeaderName| h.get(name).and_then(|v| v.to_str().ok());
        let file = page::is_file(h);
        let name = text(header::CONTENT_DISPOSITION).and_then(parse_content_disposition);
        let size = text(header::CONTENT_RANGE)
            .and_then(parse_content_range)
            .and_then(|r| r.total);
        let body = if file {
            String::new()
        } else {
            let body = page::read_body(res).await?;
            ctx.record(&final_url, status, &body);
            body
        };
        let response = parse::Response {
            status,
            final_url: &final_url,
            file,
            name,
            size,
            body: &body,
        };
        let outcome = parse::outcome(&response, &ctx.rules.gdrive, SystemTime::now())?;
        Ok((url, outcome))
    }

    /// Arquivos de uma pasta, abrindo subpastas até `MAX_FOLDER_DEPTH`.
    async fn folder_files(
        ctx: &HostCtx,
        id: &str,
        depth: u32,
        out: &mut Vec<Url>,
    ) -> Result<(), HostError> {
        let mut url = Url::parse(&ctx.rules.gdrive.folder_url)
            .map_err(|e| HostError::Changed(format!("pasta do Drive nas regras: {e}")))?;
        url.query_pairs_mut().append_pair("id", id);
        let res = page::get(ctx, &url).await?;
        if res.status == 404 {
            return Err(HostError::Offline);
        }
        for entry in parse::folder_entries(&res.body, &ctx.rules.gdrive)? {
            match entry {
                Target::File(f) => out.push(file_link(&f)?),
                Target::Folder(sub) if depth < MAX_FOLDER_DEPTH => {
                    Box::pin(Self::folder_files(ctx, &sub, depth + 1, out)).await?;
                }
                Target::Folder(_) => {}
            }
        }
        Ok(())
    }
}

/// Link "de página" de um arquivo do Drive (o que o usuário reconhece).
fn file_link(id: &str) -> Result<Url, HostError> {
    Url::parse(&format!("https://drive.google.com/file/d/{id}/view"))
        .map_err(|e| HostError::Changed(e.to_string()))
}

#[async_trait]
impl HostPlugin for Gdrive {
    fn id(&self) -> &'static str {
        "gdrive"
    }

    fn matches(&self, url: &Url, rules: &Rules) -> bool {
        parse::target(url, &rules.gdrive.hosts).is_some()
    }

    async fn check(&self, ctx: &HostCtx, url: &Url) -> Result<FileInfo, HostError> {
        match Self::target(ctx, url)? {
            Target::File(id) => {
                let (_, outcome) = Self::probe(ctx, &id).await?;
                let (name, size) = match outcome {
                    Outcome::File { name, size } | Outcome::Confirm { name, size, .. } => {
                        (name, size)
                    }
                };
                Ok(FileInfo { name, size })
            }
            Target::Folder(_) => Ok(FileInfo {
                name: None,
                size: None,
            }),
        }
    }

    async fn resolve(
        &self,
        ctx: &HostCtx,
        url: &Url,
        _attempt: u32,
    ) -> Result<Resolved, HostError> {
        let Target::File(id) = Self::target(ctx, url)? else {
            return Err(HostError::Changed(
                "é uma pasta do Drive: adicione de novo para abrir os arquivos".into(),
            ));
        };
        let (download, outcome) = Self::probe(ctx, &id).await?;
        let (direct, name, size) = match outcome {
            Outcome::File { name, size } => (download, name, size),
            Outcome::Confirm { url, name, size } => (url, name, size),
        };
        Ok(Resolved {
            url: direct,
            headers: Vec::new(),
            file_name: name,
            size,
            integrity: None,
            range: RangeStyle::Probe,
            max_connections: ctx.rules.gdrive.max_connections.max(1),
            resumable: true,
            expires_at: None,
            host_key: self.id().to_owned(),
        })
    }

    async fn expand(&self, ctx: &HostCtx, url: &Url) -> Result<Option<Vec<Url>>, HostError> {
        let Target::Folder(id) = Self::target(ctx, url)? else {
            return Ok(None);
        };
        let mut files = Vec::new();
        Self::folder_files(ctx, &id, 1, &mut files).await?;
        Ok(Some(files))
    }
}
