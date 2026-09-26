//! Notificações do sistema a partir dos avisos do serviço (fila terminou,
//! download falhou, captcha pendente), respeitando a preferência "Avisos"
//! das Opções.

use std::sync::Arc;
use swoop_api::{Backend, Notice, Push};
use swoop_service::Service;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

/// Ouve os avisos do serviço enquanto o app estiver aberto.
pub fn spawn(app: AppHandle, service: Arc<Service>) {
    tauri::async_runtime::spawn(async move {
        let mut sub = service.subscribe();
        while let Some(push) = sub.next().await {
            let Push::Notice(notice) = push else { continue };
            if !service.settings().notify {
                continue;
            }
            let (title, body) = text(&notice);
            if let Err(e) = app.notification().builder().title(title).body(body).show() {
                tracing::warn!("não consegui mostrar a notificação: {e}");
            }
        }
    });
}

/// Título e texto da notificação.
fn text(notice: &Notice) -> (String, String) {
    match notice {
        Notice::QueueFinished { completed, failed } => {
            let files = plural(*completed, "arquivo baixado", "arquivos baixados");
            let body = match failed {
                0 => files,
                n => format!("{files}, {n} com falha"),
            };
            ("Downloads concluídos".into(), body)
        }
        Notice::Failed { name, message } => {
            ("Download falhou".into(), format!("{name}: {message}"))
        }
        Notice::CaptchaNeeded { name, host } => (
            "Captcha pendente".into(),
            format!("{name} ({host}): clique em Resolver no Swoop"),
        ),
    }
}

fn plural(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textos_em_portugues() {
        let (title, body) = text(&Notice::QueueFinished {
            completed: 3,
            failed: 1,
        });
        assert_eq!(title, "Downloads concluídos");
        assert_eq!(body, "3 arquivos baixados, 1 com falha");
        let (_, body) = text(&Notice::QueueFinished {
            completed: 1,
            failed: 0,
        });
        assert_eq!(body, "1 arquivo baixado");
    }
}
