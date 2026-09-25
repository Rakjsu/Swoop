//! Preferências do usuário. Padrões aqui; os valores do usuário ficam na
//! tabela `settings` (JSON) ou vêm da linha de comando.
//!
//! As pastas `None` são preenchidas pelo serviço com as do sistema + `Swoop`
//! (Downloads, Vídeos, Músicas) antes de chegar ao motor.

use crate::kind::FileKind;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Preferências do motor e do app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(default)]
pub struct Settings {
    /// Pasta dos arquivos que não são vídeo nem áudio (e de tudo, se as
    /// outras faltarem).
    pub download_dir: Option<PathBuf>,
    /// Pasta automática dos vídeos.
    pub video_dir: Option<PathBuf>,
    /// Pasta automática das músicas e outros áudios.
    pub music_dir: Option<PathBuf>,
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
    /// Avisos do sistema quando a fila termina ou um download falha.
    pub notify: bool,
    /// Coletor: links que ficam online entram na fila sem clique.
    pub auto_start: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: None,
            video_dir: None,
            music_dir: None,
            max_active_downloads: 3,
            connections_per_download: 8,
            max_connections_per_host: 16,
            speed_limit_bps: None,
            max_retries: 5,
            notify: true,
            auto_start: false,
        }
    }
}

impl Settings {
    /// Pasta automática para o tipo de arquivo (cai na de downloads quando a
    /// específica não está definida).
    pub fn dir_for(&self, kind: FileKind) -> Option<&Path> {
        let specific = match kind {
            FileKind::Video => self.video_dir.as_deref(),
            FileKind::Audio => self.music_dir.as_deref(),
            FileKind::Other => None,
        };
        specific.or(self.download_dir.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pasta_por_tipo_com_recuo_para_downloads() {
        let mut s = Settings {
            download_dir: Some("/d".into()),
            video_dir: Some("/v".into()),
            ..Settings::default()
        };
        assert_eq!(s.dir_for(FileKind::Video), Some(Path::new("/v")));
        assert_eq!(s.dir_for(FileKind::Audio), Some(Path::new("/d")));
        assert_eq!(s.dir_for(FileKind::Other), Some(Path::new("/d")));
        s.download_dir = None;
        assert_eq!(s.dir_for(FileKind::Other), None);
    }

    #[test]
    fn json_antigo_sem_campos_novos_usa_padroes() {
        let s: Settings = serde_json::from_str(r#"{"max_active_downloads": 2}"#).unwrap();
        assert_eq!(s.max_active_downloads, 2);
        assert!(s.notify);
        assert_eq!(s.video_dir, None);
    }
}
