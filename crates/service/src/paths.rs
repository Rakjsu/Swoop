//! Pastas padrão por ambiente.
//!
//! - dados: `SWOOP_DATA_DIR` ou a pasta local de dados do SO + identificador
//!   do app (a mesma que o Tauri usa: no Windows,
//!   `%LOCALAPPDATA%\io.github.rakjsu.swoop`);
//! - downloads: preferência do usuário ou `Downloads/Swoop`.

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

/// Pasta onde os arquivos baixados vão parar.
pub fn default_download_dir(settings: &Settings) -> PathBuf {
    settings.download_dir.clone().unwrap_or_else(|| {
        dirs::download_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(std::env::temp_dir)
            .join(APP_NAME)
    })
}
