//! Janela de captcha (uma por vez). Abre a página do site que pediu, troca o
//! conteúdo pelo widget oficial (`page.js`, antes dos scripts do site) e
//! entrega a resposta ao serviço.
//!
//! Proteções: a janela não consta em nenhuma capability (a página do site
//! não chama comando nenhum), `route` só deixa navegar para o site e os
//! provedores de captcha, popups e downloads são negados. O token nunca vai
//! para o log.

mod route;

use route::{Route, route};
use std::sync::Arc;
use swoop_core::{CaptchaChallenge, DownloadId};
use swoop_service::Service;
use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// Prefixo do rótulo das janelas de captcha (`captcha-<id>`).
const PREFIX: &str = "captcha-";
/// Script que monta a página mínima.
const PAGE_JS: &str = include_str!("page.js");
/// O widget aparece até 90 s antes do fim do contador (o token vence em ~2 min).
const RENDER_AHEAD_MS: i64 = 90_000;

/// Abre (ou traz para a frente) a janela do captcha do download `id`,
/// fechando a de outro download.
pub fn open(
    app: &AppHandle,
    service: Arc<Service>,
    id: DownloadId,
    challenge: &CaptchaChallenge,
) -> tauri::Result<()> {
    let label = format!("{PREFIX}{}", id.0);
    for (other, window) in app.webview_windows() {
        if other == label {
            window.unminimize()?;
            window.show()?;
            return window.set_focus();
        }
        if other.starts_with(PREFIX) {
            window.destroy()?;
        }
    }
    let site = challenge.page_url.clone();
    let handle = app.clone();
    let url_label = label.clone();
    WebviewWindowBuilder::new(app, &label, WebviewUrl::External(site.clone()))
        .title(format!("Swoop — captcha de {}", challenge.host()))
        .inner_size(460.0, 620.0)
        .center()
        .focused(true)
        .initialization_script(script(id, challenge))
        .on_navigation(move |url| match route(url, &site, id.0) {
            Route::Allow => true,
            Route::Token(token) => {
                finish(&handle, &url_label, Some((service.clone(), id, token)));
                false
            }
            Route::Close => {
                finish(&handle, &url_label, None);
                false
            }
            Route::Deny => {
                tracing::debug!(host = url.host_str(), "captcha: navegação negada");
                false
            }
        })
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_download(|_, _| false)
        .build()?;
    Ok(())
}

/// Script de inicialização com a configuração embutida (JSON, sem risco de
/// injeção: as strings saem escapadas pelo `serde_json`).
fn script(id: DownloadId, c: &CaptchaChallenge) -> String {
    let cfg = serde_json::json!({
        "id": id.0,
        "host": c.host(),
        "kind": c.kind,
        "render_at_ms": c.not_before_ms - RENDER_AHEAD_MS,
    });
    PAGE_JS.replacen("SWOOP_CFG", &cfg.to_string(), 1)
}

/// Entrega a resposta (se houver) e fecha a janela.
fn finish(app: &AppHandle, label: &str, answer: Option<(Arc<Service>, DownloadId, String)>) {
    let app = app.clone();
    let label = label.to_owned();
    tauri::async_runtime::spawn(async move {
        if let Some((service, id, token)) = answer
            && let Err(e) = service.captcha_solved(id, token).await
        {
            tracing::warn!(id = id.0, "resposta do captcha recusada: {e}");
        }
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.destroy();
        }
    });
}

/// Janela de captcha? (o X dela fecha de verdade, não vai para a bandeja).
pub fn is_captcha(label: &str) -> bool {
    label.starts_with(PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use swoop_core::CaptchaKind;
    use tauri::Url;

    #[test]
    fn configuracao_entra_uma_vez_e_escapada() {
        let c = CaptchaChallenge {
            kind: CaptchaKind::Html {
                html: "</script><script>alert(1)</script>\"'".into(),
            },
            page_url: Url::parse("https://fastfile.cc/abcdefgh1234").unwrap(),
            action: Url::parse("https://fastfile.cc/abcdefgh1234").unwrap(),
            fields: vec![],
            answer_field: "code".into(),
            not_before_ms: 100_000,
        };
        let js = script(DownloadId(3), &c);
        assert!(!js.contains("SWOOP_CFG"));
        assert!(js.trim_end().ends_with(r#""render_at_ms":10000});"#));
        assert!(js.contains(r#""html":"</script><script>alert(1)</script>\"'""#));
    }

    #[test]
    fn janela_de_captcha_fora_das_capabilities() {
        let caps: serde_json::Value =
            serde_json::from_str(include_str!("../../capabilities/main.json")).unwrap();
        assert_eq!(caps["windows"], serde_json::json!(["main"]));
        assert!(
            caps.get("remote").is_none(),
            "página remota não fala com o Rust"
        );
    }
}
