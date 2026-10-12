//! Rapidgator premium, parte pura: respostas da API v2
//! (`{"response": …, "status": 200, "details": …}`) viram dados ou
//! `HostError` claros. O token nunca sai daqui para log.

use super::RapidgatorRules;
use serde_json::Value;
use std::time::{Duration, SystemTime};
use swoop_core::{AccountInfo, HostError, WaitReason};
use url::Url;

/// Resposta que não deu certo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// Sessão vencida (401): entrar de novo uma vez.
    Relogin,
    Error(HostError),
}

/// `details` da resposta, em minúsculas.
fn details(json: &Value) -> String {
    json.get("details")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase()
}

/// Espera a partir de agora.
fn wait(minutes: u64, reason: WaitReason) -> HostError {
    HostError::Wait {
        until: SystemTime::now() + Duration::from_secs(minutes * 60),
        reason,
    }
}

/// Lê o envelope; `Ok` = o `response`.
pub fn response(http: u16, body: &str, rules: &RapidgatorRules) -> Result<Value, Refused> {
    let json: Value = serde_json::from_str(body.trim()).map_err(|_| {
        Refused::Error(HostError::Changed(format!(
            "resposta da API do Rapidgator não é JSON (HTTP {http})"
        )))
    })?;
    let status = json
        .get("status")
        .and_then(Value::as_u64)
        .unwrap_or(u64::from(http));
    let text = details(&json);
    let has = |m: &[String]| m.iter().any(|x| text.contains(x.as_str()));
    if status == 200 {
        return json
            .get("response")
            .cloned()
            .filter(|r| !r.is_null())
            .ok_or_else(|| {
                Refused::Error(HostError::Changed("API do Rapidgator sem resposta".into()))
            });
    }
    let account = |m: &str| Refused::Error(HostError::Account(m.into()));
    Err(match status {
        _ if has(&rules.api_bad_login_markers) => {
            account("o Rapidgator recusou o e-mail ou a senha")
        }
        _ if has(&rules.api_unusual_ip_markers) => account(
            "o Rapidgator pediu para confirmar o acesso (IP novo): veja o e-mail da conta e tente de novo",
        ),
        _ if has(&rules.api_2fa_markers) => account(
            "a conta do Rapidgator usa verificação em duas etapas, que o Swoop ainda não aceita",
        ),
        _ if has(&rules.api_locked_markers) => {
            account("o Rapidgator bloqueou a conta; fale com o suporte do site")
        }
        _ if has(&rules.api_wait_markers) => {
            Refused::Error(wait(rules.api_wait_minutes, WaitReason::Backoff))
        }
        401 => Refused::Relogin,
        404 => Refused::Error(HostError::Offline),
        423 => Refused::Error(wait(rules.traffic_wait_minutes, WaitReason::Quota)),
        503 => Refused::Error(wait(rules.unavailable_wait_minutes, WaitReason::Backoff)),
        _ => Refused::Error(HostError::Changed(format!(
            "a API do Rapidgator respondeu {status}"
        ))),
    })
}

/// Token do login.
pub fn token(resp: &Value) -> Option<String> {
    resp.get("token")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
}

/// Situação da conta (`user` do login ou do `user/info`).
pub fn info(resp: &Value, now_ms: i64) -> AccountInfo {
    let user = resp.get("user").unwrap_or(resp);
    let until = user
        .get("premium_end_time")
        .and_then(Value::as_i64)
        .filter(|t| *t > 0)
        .map(|t| t * 1000);
    let flag = user
        .get("is_premium")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    AccountInfo {
        premium: flag && until.is_none_or(|u| u > now_ms),
        premium_until_ms: until,
        traffic_left: user.pointer("/traffic/left").and_then(num),
    }
}

/// Número que vem como número ou texto.
fn num(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str()?.trim().parse().ok())
}

/// Arquivo pela API: nome, tamanho, MD5 e se é pago ou só para assinantes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMeta {
    pub name: Option<String>,
    pub size: Option<u64>,
    pub md5: Option<String>,
    pub paid: bool,
}

/// `file/info` → dados do arquivo.
pub fn file_meta(resp: &Value, rules: &RapidgatorRules) -> FileMeta {
    let file = resp.get("file").unwrap_or(resp);
    let label = file
        .get("mode_label")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();
    FileMeta {
        name: file.get("name").and_then(Value::as_str).map(str::to_owned),
        size: file.get("size").and_then(num),
        md5: file
            .get("hash")
            .and_then(Value::as_str)
            .filter(|h| h.len() == 32 && h.chars().all(|c| c.is_ascii_hexdigit()))
            .map(str::to_ascii_lowercase),
        paid: rules
            .paid_markers
            .iter()
            .any(|m| label.contains(m.as_str())),
    }
}

/// `file/download` → link premium (com `delay`, espera primeiro).
pub fn download_url(resp: &Value) -> Result<Url, HostError> {
    if let Some(secs) = resp.get("delay").and_then(num).filter(|d| *d > 0) {
        return Err(HostError::Wait {
            until: SystemTime::now() + Duration::from_secs(secs),
            reason: WaitReason::Backoff,
        });
    }
    resp.get("download_url")
        .and_then(Value::as_str)
        .and_then(|u| Url::parse(u).ok())
        .filter(|u| matches!(u.scheme(), "http" | "https"))
        .ok_or_else(|| HostError::Changed("API do Rapidgator sem o link premium".into()))
}

#[cfg(test)]
#[path = "api_tests.rs"]
mod tests;
