//! Rapidgator premium, rede: login na API v2 por POST (a senha nunca vai na
//! URL; o token fica só na memória), conferência do arquivo (pago ou só de
//! assinantes ficam de fora: não é para isso que a conta serve) e o link
//! premium, com várias conexões e retomada.

use super::api::{self, Refused};
use super::secret::redact;
use super::{HOST, free};
use crate::accounts::Session;
use crate::flow::{now_ms, session_resolved};
use crate::page::{self, Page};
use crate::plugin::HostCtx;
use serde_json::Value;
use std::time::{Duration, Instant};
use swoop_core::{Account, AccountInfo, HostError, Integrity, Resolved, Secret};
use url::Url;

/// Endereço de um método da API.
fn api_url(ctx: &HostCtx, method: &str, params: &[(&str, &str)]) -> Result<Url, HostError> {
    let rules = &ctx.rules.rapidgator;
    let mut url = free::origin(ctx)?
        .join(&rules.api_path)
        .and_then(|base| base.join(method))
        .map_err(|_| HostError::Changed(format!("caminho da API inválido: {method}")))?;
    url.query_pairs_mut().extend_pairs(params);
    Ok(url)
}

/// Resposta da API (corpo redigido em qualquer gravação).
fn read(ctx: &HostCtx, step: &str, page: &Page) -> Result<Value, Refused> {
    api::response(page.status, &page.body, &ctx.rules.rapidgator).map_err(|r| match r {
        Refused::Error(e) => Refused::Error(ctx.changed(HOST, step, &redact(&page.body), e)),
        r => r,
    })
}

/// Erro de API fora do login: 401 depois do novo login é falha da conta.
fn fail(r: Refused) -> HostError {
    match r {
        Refused::Relogin => HostError::Account("o Rapidgator recusou a sessão da conta".into()),
        Refused::Error(e) => e,
    }
}

/// Entra com a conta (POST) e lê a situação dela; guarda a sessão.
async fn login(
    ctx: &HostCtx,
    key: &str,
    acc: &Account,
) -> Result<(Session, AccountInfo), HostError> {
    let lock = ctx.accounts.login_lock();
    let _one = lock.lock().await;
    let fields = [
        ("login".to_owned(), acc.username.clone()),
        ("password".to_owned(), acc.secret.expose().to_owned()),
    ];
    let url = api_url(ctx, &ctx.rules.rapidgator.api_login, &[])?;
    let res = page::post_form_with(ctx, &url, &fields, Some(redact)).await?;
    let resp = read(ctx, "login", &res).map_err(|r| match r {
        Refused::Relogin => HostError::Account("o Rapidgator recusou o e-mail ou a senha".into()),
        Refused::Error(e) => e,
    })?;
    let token = api::token(&resp)
        .ok_or_else(|| HostError::Changed("login do Rapidgator sem o token".into()))?;
    let info = if resp.get("user").is_some() {
        api::info(&resp, now_ms())
    } else {
        let url = api_url(
            ctx,
            &ctx.rules.rapidgator.api_user_info,
            &[("token", &token)],
        )?;
        let res = page::get_from(ctx, &url, &url, false, Some(redact)).await?;
        api::info(&read(ctx, "conta", &res).map_err(fail)?, now_ms())
    };
    tracing::info!(premium = info.premium, "rapidgator: login da conta");
    let session = Session {
        token: Secret::new(token),
        premium: info.premium,
        opened: Instant::now(),
    };
    ctx.accounts
        .set_session(key, &acc.username, session.clone());
    Ok((session, info))
}

/// Sessão em uso ou um login novo (vencida pelas regras ou `fresh`).
async fn session(
    ctx: &HostCtx,
    key: &str,
    acc: &Account,
    fresh: bool,
) -> Result<Session, HostError> {
    let ttl = Duration::from_secs(ctx.rules.rapidgator.session_minutes * 60);
    match ctx.accounts.session(key) {
        Some(s) if !fresh && s.opened.elapsed() < ttl => Ok(s),
        _ => Ok(login(ctx, key, acc).await?.0),
    }
}

/// Chama um método com o token; sessão vencida (401) = um login novo.
async fn call(
    ctx: &HostCtx,
    key: &str,
    acc: &Account,
    method: &str,
    step: &str,
    file_id: &str,
) -> Result<Value, HostError> {
    let mut fresh = false;
    loop {
        let s = session(ctx, key, acc, fresh).await?;
        let url = api_url(
            ctx,
            method,
            &[("token", s.token.expose()), ("file_id", file_id)],
        )?;
        let res = page::get_from(ctx, &url, &url, false, Some(redact)).await?;
        match read(ctx, step, &res) {
            Ok(v) => return Ok(v),
            Err(Refused::Relogin) if !fresh => {
                tracing::info!("rapidgator: sessão vencida; entrando de novo");
                fresh = true;
            }
            Err(r) => return Err(fail(r)),
        }
    }
}

/// Link premium do arquivo `file_id`; `None` = a conta não é premium (o
/// download segue pelo grátis).
pub async fn resolve(
    ctx: &HostCtx,
    key: &str,
    acc: &Account,
    file_id: &str,
    file_url: &Url,
) -> Result<Option<Resolved>, HostError> {
    let rules = &ctx.rules.rapidgator;
    let key = key.to_owned();
    if !session(ctx, &key, acc, false).await?.premium {
        tracing::info!("rapidgator: conta sem premium; indo pelo grátis");
        return Ok(None);
    }
    let meta = api::file_meta(
        &call(ctx, &key, acc, &rules.api_file_info, "arquivo", file_id).await?,
        rules,
    );
    if meta.paid {
        return Err(HostError::Account(
            "este arquivo do Rapidgator é pago (ou só para assinantes de quem enviou): a conta premium não baixa".into(),
        ));
    }
    let link =
        api::download_url(&call(ctx, &key, acc, &rules.api_file_download, "link", file_id).await?)?;
    tracing::info!("rapidgator: link premium");
    let mut r = session_resolved(
        ctx,
        link,
        file_url,
        rules.premium_connections,
        rules.premium_resumable,
        HOST.into(),
    );
    r.file_name = meta.name;
    r.size = meta.size;
    r.integrity = meta.md5.map(Integrity::Md5);
    Ok(Some(r))
}

/// Confere a conta no site (aba Contas): sempre um login novo.
pub async fn info(ctx: &HostCtx, key: &str, acc: &Account) -> Result<AccountInfo, HostError> {
    Ok(login(ctx, key, acc).await?.1)
}
