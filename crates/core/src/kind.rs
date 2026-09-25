//! Tipo do arquivo pela extensão do nome final, para escolher a pasta
//! automática (vídeos, músicas ou downloads). Compactados (.rar, .zip…)
//! ficam em "outros": a extração decide depois onde vai o conteúdo.

/// Categoria usada para escolher a pasta de destino.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Video,
    Audio,
    Other,
}

const VIDEO: &[&str] = &[
    "3gp", "avi", "divx", "f4v", "flv", "m2ts", "m4v", "mkv", "mov", "mp4", "mpeg", "mpg", "mts",
    "ogv", "rm", "rmvb", "ts", "vob", "webm", "wmv",
];

const AUDIO: &[&str] = &[
    "aac", "aif", "aiff", "alac", "ape", "dsf", "flac", "m4a", "m4b", "mka", "mp3", "oga", "ogg",
    "opus", "wav", "wma", "wv",
];

/// Categoria do arquivo pela extensão (sem diferenciar maiúsculas).
pub fn file_kind(name: &str) -> FileKind {
    let Some((_, ext)) = name.rsplit_once('.') else {
        return FileKind::Other;
    };
    let ext = ext.to_ascii_lowercase();
    if VIDEO.contains(&ext.as_str()) {
        FileKind::Video
    } else if AUDIO.contains(&ext.as_str()) {
        FileKind::Audio
    } else {
        FileKind::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifica_pela_extensao() {
        assert_eq!(file_kind("Filme.2024.1080p.MKV"), FileKind::Video);
        assert_eq!(file_kind("aula.mp4"), FileKind::Video);
        assert_eq!(file_kind("album - 01.flac"), FileKind::Audio);
        assert_eq!(file_kind("podcast.MP3"), FileKind::Audio);
        assert_eq!(file_kind("filme.part1.rar"), FileKind::Other);
        assert_eq!(file_kind("setup.exe"), FileKind::Other);
        assert_eq!(file_kind("sem_extensao"), FileKind::Other);
        assert_eq!(file_kind(".mp4"), FileKind::Video);
    }
}
