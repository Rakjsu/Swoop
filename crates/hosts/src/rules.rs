//! Regras dos servidores: `rules/hosts.toml` embutido, com o arquivo do
//! usuário (`<dados>/rules/hosts.toml`) mesclado por cima, chave a chave.
//! Assim um seletor ou endereço que mudou no site se corrige sem recompilar.

use serde::Deserialize;
use std::path::Path;

/// Regras embutidas no binário.
const EMBEDDED: &str = include_str!("../../../rules/hosts.toml");
/// Override do usuário, relativo à pasta de dados.
pub const OVERRIDE_FILE: &str = "rules/hosts.toml";

/// Todas as regras.
#[derive(Debug, Clone, Deserialize)]
pub struct Rules {
    pub version: u32,
    pub user_agent: String,
    pub pixeldrain: PixeldrainRules,
    pub mediafire: MediafireRules,
    pub gdrive: GdriveRules,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PixeldrainRules {
    pub hosts: Vec<String>,
    pub api: String,
    pub max_connections: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MediafireRules {
    pub hosts: Vec<String>,
    pub api: String,
    pub download_selector: String,
    pub scrambled_attr: String,
    pub name_selector: String,
    pub captcha_markers: Vec<String>,
    pub danger_markers: Vec<String>,
    pub password_markers: Vec<String>,
    pub max_connections: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GdriveRules {
    pub hosts: Vec<String>,
    pub download_url: String,
    pub confirm_form_selector: String,
    pub confirm_name_selector: String,
    pub confirm_size_selector: String,
    pub folder_url: String,
    pub folder_entry_selector: String,
    pub quota_markers: Vec<String>,
    pub missing_markers: Vec<String>,
    pub quota_wait_minutes: u64,
    pub max_connections: u16,
}

/// Erro ao ler as regras.
#[derive(Debug, thiserror::Error)]
pub enum RulesError {
    #[error("regras inválidas: {0}")]
    Invalid(String),
}

impl Rules {
    /// Só as regras embutidas.
    pub fn embedded() -> Self {
        Self::from_tables(parse(EMBEDDED).expect("rules/hosts.toml embutido é válido"))
            .expect("rules/hosts.toml embutido é completo")
    }

    /// Regras da pasta de dados (`<dados>/rules/hosts.toml` por cima das
    /// embutidas).
    pub fn for_data_dir(data_dir: &Path) -> Self {
        Self::load(&data_dir.join(OVERRIDE_FILE))
    }

    /// Embutidas + override do usuário, se o arquivo existir. Override
    /// ilegível é ignorado com aviso (o app continua com as embutidas).
    pub fn load(override_path: &Path) -> Self {
        match std::fs::read_to_string(override_path) {
            Ok(text) => Self::with_override(&text).unwrap_or_else(|e| {
                tracing::warn!(
                    "{}: {e}; usando as regras embutidas",
                    override_path.display()
                );
                Self::embedded()
            }),
            Err(_) => Self::embedded(),
        }
    }

    /// Embutidas com o texto `override_toml` mesclado por cima.
    pub fn with_override(override_toml: &str) -> Result<Self, RulesError> {
        let mut base = parse(EMBEDDED)?;
        merge(&mut base, parse(override_toml)?);
        Self::from_tables(base)
    }

    fn from_tables(t: toml::Table) -> Result<Self, RulesError> {
        t.try_into()
            .map_err(|e: toml::de::Error| RulesError::Invalid(e.to_string()))
    }

    /// O host (sem `www.`/subdomínio) está na lista?
    pub fn host_in(list: &[String], host: &str) -> bool {
        let host = host.to_ascii_lowercase();
        list.iter()
            .any(|h| host == *h || host.ends_with(&format!(".{h}")))
    }
}

fn parse(text: &str) -> Result<toml::Table, RulesError> {
    text.parse::<toml::Table>()
        .map_err(|e| RulesError::Invalid(e.to_string()))
}

/// Mescla `over` em `base`: tabelas recursivamente, o resto substitui.
fn merge(base: &mut toml::Table, over: toml::Table) {
    for (key, value) in over {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(b)), toml::Value::Table(o)) => merge(b, o),
            (_, v) => {
                base.insert(key, v);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embutidas_carregam() {
        let r = Rules::embedded();
        assert_eq!(r.version, 1);
        assert_eq!(r.pixeldrain.max_connections, 1);
        assert!(r.gdrive.quota_wait_minutes > 0);
    }

    #[test]
    fn override_muda_so_o_que_traz() {
        let r = Rules::with_override(
            "[mediafire]\ndownload_selector = \"a.outro\"\n[pixeldrain]\nmax_connections = 3\n",
        )
        .unwrap();
        assert_eq!(r.mediafire.download_selector, "a.outro");
        assert_eq!(r.mediafire.scrambled_attr, "data-scrambled-url");
        assert_eq!(r.pixeldrain.max_connections, 3);
        assert!(Rules::with_override("isso não é toml [").is_err());
    }

    #[test]
    fn override_ilegivel_cai_nas_embutidas() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hosts.toml");
        std::fs::write(&path, "[pixeldrain]\nmax_connections = \"muitas\"\n").unwrap();
        assert_eq!(Rules::load(&path).pixeldrain.max_connections, 1);
        assert_eq!(Rules::load(&dir.path().join("nao-existe.toml")).version, 1);
    }

    #[test]
    fn host_com_subdominio() {
        let list = vec!["mediafire.com".to_owned()];
        assert!(Rules::host_in(&list, "www.MediaFire.com"));
        assert!(Rules::host_in(&list, "mediafire.com"));
        assert!(!Rules::host_in(&list, "notmediafire.com"));
    }
}
