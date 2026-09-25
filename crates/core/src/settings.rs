//! Preferências do usuário que afetam o motor. Padrões aqui; valores do
//! usuário ficam na tabela `settings` (fase 2) ou vêm da linha de comando.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Preferências do motor de download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Pasta padrão dos downloads (`None` = pasta Downloads do SO + `Swoop`).
    pub download_dir: Option<PathBuf>,
    /// Downloads baixando ao mesmo tempo.
    pub max_active_downloads: usize,
    /// Conexões (segmentos) por arquivo.
    pub connections_per_download: u16,
    /// Conexões somadas de todos os downloads de um mesmo servidor.
    pub max_connections_per_host: u16,
    /// Limite global em bytes/s (`None` = sem limite).
    pub speed_limit_bps: Option<u64>,
    /// Tentativas seguidas sem progresso antes de desistir.
    pub max_retries: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: None,
            max_active_downloads: 3,
            connections_per_download: 8,
            max_connections_per_host: 16,
            speed_limit_bps: None,
            max_retries: 5,
        }
    }
}
