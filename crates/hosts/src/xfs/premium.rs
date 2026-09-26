//! XFileSharing premium, rede: link pela API (chave) ou pela página do arquivo
//! com a sessão do login; conferência da conta. Senha e chave só vão ao
//! próprio site (a URL com a chave nunca aparece em erro: `page::network`).

use super::account::{self, Premium};
use super::{now_ms, site_key};
use crate::page;
use crate::plugin::HostCtx;
use std::time::SystemTime;
use swoop_core::{Account, AccountInfo, AccountKind, HostError};
use url::Url;

/// Começo do site (`https://fastfile.cc/`), mantendo esquema e porta.
fn origin(url: &Url) -> Url {
    let mut o = url.clone();
    o.set_path("/");
    o.set_query(None);
    o.set_fragment(None);
    o
}

/// Caminho das regras dentro do site.
fn at(origin: &Url, path: &str) -> Result<Url, HostError> {
    origin
        .join(path)
        .map_err(|_| HostError::Changed(format!("caminho inválido nas regras: {path}")))
}

/// Endereço da API com a chave.
fn api(origin: &Url, path: &str, params: &[(&str, &str)]) -> Result<Url, HostError> {
    let mut url = at(origin, path)?;
    url.query_pairs_mut().extend_pairs(params);
    Ok(url)
}

/// Link premium do arquivo `code`.
pub async fn link(ctx: &HostCtx, url: &Url, code: &str, acc: &Account) -> Result<Url, HostError> {
    let rules = &ctx.rules.xfs;
    let host = site_key(url);
    let origin = origin(url);
    match acc.kind {
        AccountKind::ApiKey => {
            let params = [("key", acc.secret.expose()), ("file_code", code)];
            let res = page::get(ctx, &api(&origin, &rules.api_direct_link, &params)?).await?;
            account::api_link(&res.body, &host, &origin)
        }
        AccountKind::Login => {
            login(ctx, &origin, &host, acc).await?;
            from_page(ctx, url, &host).await
        }
    }
}

/// Página do arquivo com a sessão premium → link (direto, na página ou
/// depois do formulário premium).
async fn from_page(ctx: &HostCtx, url: &Url, host: &str) -> Result<Url, HostError> {
    let rules = &ctx.rules.xfs;
    let first = page::get(ctx, url).await?;
    if first.file {
        return Ok(first.url);
    }
    let form = match account::premium_page(&first.body, &first.url, rules, host, SystemTime::now())?
    {
        Premium::Link(link) => return Ok(link),
        Premium::Form(form) => form,
    };
    let res = page::post_form(ctx, &form.target(&first.url), &form.fields).await?;
    if res.file {
        return Ok(res.url);
    }
    match account::premium_page(&res.body, &res.url, rules, host, SystemTime::now())? {
        Premium::Link(link) => Ok(link),
        Premium::Form(_) => Err(HostError::Changed(format!(
            "o {host} não entregou o link premium"
        ))),
    }
}

/// Entra no site com usuário e senha; a sessão fica nos cookies.
async fn login(ctx: &HostCtx, origin: &Url, host: &str, acc: &Account) -> Result<(), HostError> {
    let rules = &ctx.rules.xfs;
    let fields = [
        ("op", "login"),
        ("login", acc.username.as_str()),
        ("password", acc.secret.expose()),
        ("redirect", ""),
    ]
    .map(|(k, v)| (k.to_owned(), v.to_owned()));
    let res = page::post_form(ctx, &at(origin, &rules.login_path)?, &fields).await?;
    account::login_result(&res.body, rules, host)
}

/// Confere a conta do servidor `host_key`: pela API (chave) ou pela página
/// da conta (login).
pub async fn info(ctx: &HostCtx, host_key: &str, acc: &Account) -> Result<AccountInfo, HostError> {
    let rules = &ctx.rules.xfs;
    let origin = Url::parse(&format!("{}://{host_key}/", rules.account_scheme))
        .map_err(|_| HostError::Unsupported)?;
    match acc.kind {
        AccountKind::ApiKey => {
            let params = [("key", acc.secret.expose())];
            let res = page::get(ctx, &api(&origin, &rules.api_account_info, &params)?).await?;
            account::api_info(&res.body, host_key, now_ms())
        }
        AccountKind::Login => {
            login(ctx, &origin, host_key, acc).await?;
            let res = page::get(ctx, &at(&origin, &rules.account_page)?).await?;
            Ok(account::page_info(&res.body, rules, now_ms()))
        }
    }
}
