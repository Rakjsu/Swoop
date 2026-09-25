//! Leitura da release do GitHub (pura) e escolha do instalador.

use crate::{Source, UpdateError, Version};
use serde::Deserialize;

/// Arquivo anexado a uma release.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Asset {
    pub name: String,
    #[serde(rename = "browser_download_url")]
    pub url: String,
    pub size: u64,
}

/// Campos usados da resposta `GET /repos/{dono}/{repo}/releases/latest`.
#[derive(Debug, Clone, Deserialize)]
pub struct GhRelease {
    pub tag_name: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

/// Atualização disponível: instalador NSIS e a lista de hashes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    pub version: Version,
    pub notes: String,
    pub setup: Asset,
    pub sums: Asset,
}

/// Nome do arquivo com os hashes publicado em cada release.
pub const SUMS_NAME: &str = "SHA256SUMS.txt";

/// Nome do instalador NSIS gerado pelo Tauri para a versão.
pub fn setup_name(v: Version) -> String {
    format!("Swoop_{v}_x64-setup.exe")
}

/// Decide se a release é uma atualização válida para `current`.
pub fn select(
    release: &GhRelease,
    current: Version,
    source: &Source,
) -> Result<Option<Update>, UpdateError> {
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let version: Version = release.tag_name.parse().map_err(UpdateError::BadResponse)?;
    if version <= current {
        return Ok(None);
    }
    let find = |name: &str| {
        release
            .assets
            .iter()
            .find(|a| a.name == name)
            .cloned()
            .ok_or_else(|| UpdateError::Missing(name.to_owned()))
    };
    let setup = find(&setup_name(version))?;
    let sums = find(SUMS_NAME)?;
    for asset in [&setup, &sums] {
        if !asset.url.starts_with(&source.download_prefix) {
            return Err(UpdateError::Untrusted(asset.url.clone()));
        }
    }
    Ok(Some(Update {
        version,
        notes: release.body.clone().unwrap_or_default(),
        setup,
        sums,
    }))
}

/// Procura o sha256 (hex minúsculo) de `name` num arquivo no formato do
/// `sha256sum`: `<hash>  <nome>` (o nome pode vir com `*` na frente).
pub fn parse_sums(text: &str, name: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (hash, file) = line.trim().split_once(char::is_whitespace)?;
        let file = file.trim().trim_start_matches('*');
        (file == name && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release() -> GhRelease {
        serde_json::from_str(include_str!("../tests/fixtures/release.json")).unwrap()
    }

    fn v(s: &str) -> Version {
        s.parse().unwrap()
    }

    #[test]
    fn escolhe_instalador_e_hashes() {
        let up = select(&release(), v("0.1.0"), &Source::swoop())
            .unwrap()
            .unwrap();
        assert_eq!(up.version, v("0.2.0"));
        assert_eq!(up.setup.name, "Swoop_0.2.0_x64-setup.exe");
        assert_eq!(up.sums.name, SUMS_NAME);
        assert!(up.notes.contains("motor"));
    }

    #[test]
    fn ignora_versao_igual_ou_menor_e_rascunho() {
        assert_eq!(
            select(&release(), v("0.2.0"), &Source::swoop()).unwrap(),
            None
        );
        assert_eq!(
            select(&release(), v("1.0.0"), &Source::swoop()).unwrap(),
            None
        );
        let mut r = release();
        r.prerelease = true;
        assert_eq!(select(&r, v("0.1.0"), &Source::swoop()).unwrap(), None);
    }

    #[test]
    fn recusa_url_de_outro_repositorio() {
        let mut r = release();
        r.assets[0].url = "https://github.com/outro/repo/releases/download/v0.2.0/x.exe".into();
        assert!(matches!(
            select(&r, v("0.1.0"), &Source::swoop()),
            Err(UpdateError::Untrusted(_))
        ));
    }

    #[test]
    fn release_sem_hashes_e_recusada() {
        let mut r = release();
        r.assets.retain(|a| a.name != SUMS_NAME);
        assert!(matches!(
            select(&r, v("0.1.0"), &Source::swoop()),
            Err(UpdateError::Missing(_))
        ));
    }

    #[test]
    fn le_arquivo_de_hashes() {
        let h = "a".repeat(64);
        let text = format!(
            "{h}  Swoop_0.2.0_x64-setup.exe\r\n{}  *Swoop-Installer-0.2.0.exe\n",
            "B".repeat(64)
        );
        assert_eq!(parse_sums(&text, "Swoop_0.2.0_x64-setup.exe"), Some(h));
        assert_eq!(
            parse_sums(&text, "Swoop-Installer-0.2.0.exe"),
            Some("b".repeat(64))
        );
        assert_eq!(parse_sums(&text, "outro.exe"), None);
        assert_eq!(parse_sums("curto  x.exe", "x.exe"), None);
    }
}
