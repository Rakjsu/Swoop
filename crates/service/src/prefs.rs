//! Preferências salvas no banco (chave `engine`, em JSON): leitura com
//! recuo para os padrões, validação e gravação.

use crate::ServiceError;
use crate::paths::fill_default_dirs;
use swoop_core::Settings;
use swoop_store::{Store, settings};

/// Chave da tabela `settings` com as preferências do motor e do app.
const KEY: &str = "engine";

/// Preferências para abrir o motor: as explícitas (CLI) ou as salvas, com
/// as pastas vazias preenchidas pelas do sistema.
pub async fn load(store: &Store, explicit: Option<Settings>) -> Result<Settings, ServiceError> {
    let mut s = match explicit {
        Some(s) => s,
        None => match store.call(|c| settings::load(c, KEY)).await? {
            Some(json) => serde_json::from_str(&json).unwrap_or_else(|e| {
                tracing::warn!("preferências salvas ilegíveis ({e}); usando as padrão");
                Settings::default()
            }),
            None => Settings::default(),
        },
    };
    fill_default_dirs(&mut s);
    Ok(s)
}

/// Valida e grava; devolve o que foi salvo (já normalizado).
pub async fn save(store: &Store, settings: Settings) -> Result<Settings, ServiceError> {
    let mut s = normalize(settings)?;
    fill_default_dirs(&mut s);
    let json = serde_json::to_string(&s).map_err(|e| ServiceError::BadSettings(e.to_string()))?;
    store.call(move |c| settings::save(c, KEY, &json)).await?;
    Ok(s)
}

/// Limites sensatos e pastas: vazia = padrão do sistema; relativa é recusada
/// (dependeria da pasta em que o app foi aberto).
fn normalize(mut s: Settings) -> Result<Settings, ServiceError> {
    s.max_active_downloads = s.max_active_downloads.clamp(1, 20);
    s.connections_per_download = s.connections_per_download.clamp(1, 32);
    s.max_connections_per_host = s.max_connections_per_host.clamp(1, 64);
    s.max_retries = s.max_retries.min(50);
    s.speed_limit_bps = s.speed_limit_bps.filter(|b| *b > 0);
    for dir in [&mut s.download_dir, &mut s.video_dir, &mut s.music_dir] {
        if dir.as_ref().is_some_and(|d| d.as_os_str().is_empty()) {
            *dir = None;
        }
        if let Some(d) = dir
            && !d.is_absolute()
        {
            return Err(ServiceError::BadSettings(format!(
                "a pasta \"{}\" precisa ser um caminho completo",
                d.display()
            )));
        }
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn normaliza_limites_e_recusa_pasta_relativa() {
        let s = normalize(Settings {
            max_active_downloads: 0,
            connections_per_download: 500,
            speed_limit_bps: Some(0),
            video_dir: Some(PathBuf::new()),
            ..Settings::default()
        })
        .unwrap();
        assert_eq!(s.max_active_downloads, 1);
        assert_eq!(s.connections_per_download, 32);
        assert_eq!(s.speed_limit_bps, None);
        assert_eq!(s.video_dir, None);

        let err = normalize(Settings {
            music_dir: Some("musicas".into()),
            ..Settings::default()
        });
        assert!(matches!(err, Err(ServiceError::BadSettings(_))));
    }
}
