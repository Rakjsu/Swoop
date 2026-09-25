//! Pastas padrão por ambiente.
//!
//! - dados: `SWOOP_DATA_DIR` ou a pasta local de dados do SO + identificador
//!   do app (a mesma que o Tauri usa: no Windows,
//!   `%LOCALAPPDATA%\io.github.rakjsu.swoop`);
//! - destinos: preferência do usuário ou as pastas do sistema + `Swoop`
//!   (`Downloads`, `Vídeos`, `Músicas`).

use std::path::PathBuf;
use swoop_core::{APP_IDENTIFIER, APP_NAME, Settings};

/// Pasta de dados pedida por `SWOOP_DATA_DIR` (tem prioridade em todo lugar:
/// CLI, app e testes).
pub fn data_dir_from_env() -> Option<PathBuf> {
    std::env::var_os("SWOOP_DATA_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Pasta de dados (banco, trava, regras, logs).
pub fn default_data_dir() -> PathBuf {
    if let Some(dir) = data_dir_from_env() {
        return dir;
    }
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_IDENTIFIER)
}

/// Pasta onde vão os arquivos que não são vídeo nem áudio.
pub fn default_download_dir(settings: &Settings) -> PathBuf {
    settings.download_dir.clone().unwrap_or_else(|| {
        dirs::download_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(std::env::temp_dir)
            .join(APP_NAME)
    })
}

/// Preenche as pastas vazias com as do sistema + `Swoop`. Sem pasta de
/// vídeos ou músicas no sistema, o tipo fica sem pasta e cai na de downloads.
pub fn fill_default_dirs(settings: &mut Settings) {
    settings.download_dir = Some(default_download_dir(settings));
    if settings.video_dir.is_none() {
        settings.video_dir = dirs::video_dir().map(|d| d.join(APP_NAME));
    }
    if settings.music_dir.is_none() {
        settings.music_dir = dirs::audio_dir().map(|d| d.join(APP_NAME));
    }
}
