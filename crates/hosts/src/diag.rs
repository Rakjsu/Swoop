//! Páginas guardadas quando um plugin não entende o site
//! (`HostError::Changed`), para o usuário mandar e o plugin ser corrigido.
//! Só o corpo da página: cabeçalhos e cookies nunca. Ficam no máximo `KEEP`
//! arquivos (os mais antigos saem).

use crate::dump::clean;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Arquivos guardados no máximo.
const KEEP: usize = 20;

/// Pasta de diagnóstico (`<dados>/diagnostico`).
#[derive(Debug)]
pub struct Diagnostics {
    dir: PathBuf,
}

impl Diagnostics {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Grava `<servidor>-<etapa>-<segundos>.html` e devolve o caminho.
    /// Falha de disco só gera aviso (o erro original segue para o usuário).
    pub fn save(&self, host: &str, step: &str, body: &str) -> Option<PathBuf> {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let path = self
            .dir
            .join(format!("{}-{}-{secs}.html", clean(host), clean(step)));
        let saved = std::fs::create_dir_all(&self.dir).and_then(|_| std::fs::write(&path, body));
        match saved {
            Ok(()) => {
                prune(&self.dir);
                tracing::info!("página guardada para diagnóstico: {}", path.display());
                Some(path)
            }
            Err(e) => {
                tracing::warn!("não deu para guardar a página de diagnóstico: {e}");
                None
            }
        }
    }
}

/// Mantém só os `KEEP` arquivos mais novos.
fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    if files.len() <= KEEP {
        return;
    }
    files.sort();
    for (_, old) in &files[..files.len() - KEEP] {
        let _ = std::fs::remove_file(old);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guarda_com_nome_seguro_e_limita_a_quantidade() {
        let dir = tempfile::tempdir().unwrap();
        let d = Diagnostics::new(dir.path().join("diagnostico"));
        let path = d.save("127.0.0.1:8080", "final", "<html>x</html>").unwrap();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("127.0.0.1_8080-final-"), "{name}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "<html>x</html>");
        for i in 0..KEEP + 5 {
            std::fs::write(
                dir.path().join("diagnostico").join(format!("v{i}.html")),
                "x",
            )
            .unwrap();
        }
        d.save("h", "s", "y").unwrap();
        let left = std::fs::read_dir(dir.path().join("diagnostico"))
            .unwrap()
            .count();
        assert_eq!(left, KEEP);
    }
}
