//! Rapidgator: situações do site que param o fluxo em qualquer página
//! (Cloudflare, offline, só premium, limite de tamanho do grátis, espera
//! entre downloads, limite por hora/dia, um por vez, fora do ar).

use super::RapidgatorRules;
use super::parse::regex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use swoop_core::{HostError, WaitReason};

/// Espera do servidor inteiro a partir de `now`.
fn host_wait(now: SystemTime, minutes: u64) -> HostError {
    HostError::Wait {
        until: now + Duration::from_secs(minutes * 60 + 5),
        reason: WaitReason::HostLimit,
    }
}

/// Próxima meia-noite UTC (limite diário) com 5 min de folga.
fn next_midnight(now: SystemTime) -> SystemTime {
    let secs = now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    UNIX_EPOCH + Duration::from_secs((secs / 86_400 + 1) * 86_400 + 300)
}

/// Situações que param o fluxo em qualquer página.
pub fn blocked(
    status: u16,
    html: &str,
    rules: &RapidgatorRules,
    now: SystemTime,
) -> Result<(), HostError> {
    let lower = html.to_lowercase();
    let has = |m: &[String]| m.iter().any(|x| lower.contains(x.as_str()));
    if matches!(status, 403 | 503) && has(&rules.cloudflare_markers) {
        return Err(HostError::BrowserRequired(
            "o Rapidgator pediu a verificação do Cloudflare; abra o link no navegador e tente de novo mais tarde".into(),
        ));
    }
    if status == 404 || has(&rules.offline_markers) {
        return Err(HostError::Offline);
    }
    if has(&rules.premium_markers) {
        return Err(HostError::PremiumOnly(Some(
            "o Rapidgator só libera este arquivo para contas premium (aba Contas)".into(),
        )));
    }
    if let Some(limit) = regex(&rules.free_size_pattern)?
        .captures(&lower)
        .and_then(|c| c.get(1))
    {
        return Err(HostError::PremiumOnly(Some(format!(
            "o grátis do Rapidgator baixa arquivos de até {}; este é maior: use uma conta premium (aba Contas)",
            limit.as_str().to_uppercase()
        ))));
    }
    if let Some(min) = regex(&rules.delay_pattern)?
        .captures(&lower)
        .and_then(|c| c.get(1)?.as_str().parse::<u64>().ok())
    {
        return Err(host_wait(now, min));
    }
    if has(&rules.hourly_markers) {
        return Err(host_wait(now, rules.hourly_wait_minutes));
    }
    if has(&rules.daily_markers) {
        return Err(HostError::Wait {
            until: next_midnight(now),
            reason: WaitReason::HostLimit,
        });
    }
    if has(&rules.one_at_a_time_markers) {
        return Err(host_wait(now, rules.one_at_a_time_wait_minutes));
    }
    if has(&rules.unavailable_markers) {
        return Err(HostError::Wait {
            until: now + Duration::from_secs(rules.unavailable_wait_minutes * 60),
            reason: WaitReason::Backoff,
        });
    }
    Ok(())
}
