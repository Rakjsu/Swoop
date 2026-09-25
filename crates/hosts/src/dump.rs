//! Gravação das páginas/respostas que os plugins leem, para virar fixture
//! (`swoop-cli resolve <link> --dump-fixtures <pasta>`). Só o corpo é
//! gravado: cabeçalhos (cookies) nunca.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use url::Url;

/// Pasta de destino + contador (a ordem dos arquivos é a ordem das leituras).
#[derive(Debug)]
pub struct Dump {
    dir: PathBuf,
    next: AtomicU32,
}

impl Dump {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            next: AtomicU32::new(1),
        }
    }

    /// Grava `body` como `NN-<host>-<último trecho>-<status>.<ext>`.
    /// Falha de disco só gera aviso: o comando continua.
    pub fn save(&self, url: &Url, status: u16, body: &str) {
        let n = self.next.fetch_add(1, Ordering::SeqCst);
        let host = url.host_str().unwrap_or("host");
        let last = url
            .path_segments()
            .and_then(|mut s| s.rfind(|x| !x.is_empty()))
            .unwrap_or("raiz");
        let name = format!(
            "{n:02}-{}-{}-{status}.{}",
            clean(host),
            clean(last),
            ext(body)
        );
        let path = self.dir.join(name);
        let result = std::fs::create_dir_all(&self.dir).and_then(|_| std::fs::write(&path, body));
        match result {
            Ok(()) => tracing::info!("resposta gravada em {}", path.display()),
            Err(e) => tracing::warn!("não deu para gravar {}: {e}", path.display()),
        }
    }
}

/// Só letras, números, `.`, `_` e `-` (até 40 caracteres).
fn clean(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .take(40)
        .collect()
}

/// Extensão pelo começo do corpo.
fn ext(body: &str) -> &'static str {
    match body.trim_start().chars().next() {
        Some('{' | '[') => "json",
        Some('<') => "html",
        _ => "txt",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nome_seguro_e_extensao_pelo_conteudo() {
        let dir = tempfile::tempdir().unwrap();
        let dump = Dump::new(dir.path().join("fx"));
        let url = Url::parse("https://pixeldrain.com/api/file/abc/info?x=1").unwrap();
        dump.save(&url, 200, "{\"ok\":true}");
        let url = Url::parse("https://www.mediafire.com/file/k/n%20o.zip/file").unwrap();
        dump.save(&url, 404, "  <html></html>");
        let mut names: Vec<String> = std::fs::read_dir(dir.path().join("fx"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "01-pixeldrain.com-info-200.json",
                "02-www.mediafire.com-file-404.html"
            ]
        );
    }
}
