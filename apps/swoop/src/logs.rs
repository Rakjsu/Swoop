//! Log em arquivo (`<dados>/logs/swoop.log`) além do console: o app em
//! release não tem console, e é este arquivo que o usuário manda quando algo
//! falha. Ao abrir, um log maior que `MAX` vira `swoop.old.log`. Nada de URL,
//! cookie ou segredo chega aqui (regra de todos os crates).

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::Mutex;

/// Tamanho a partir do qual o log é trocado ao abrir.
const MAX: u64 = 5 * 1024 * 1024;

/// Arquivo aberto por `attach` (antes disso, só o console).
static FILE: Mutex<Option<File>> = Mutex::new(None);

/// Escritor do `tracing`: console e, se aberto, o arquivo.
pub struct Tee;

impl Write for Tee {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let _ = io::stderr().write_all(buf);
        if let Some(file) = FILE.lock().expect("log").as_mut() {
            let _ = file.write_all(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Passa a gravar também em `<dir>/swoop.log`. Falha só gera aviso.
pub fn attach(dir: &Path) {
    match open(dir) {
        Ok(path) => tracing::info!("log em {}", path.display()),
        Err(e) => tracing::warn!("sem log em arquivo: {e}"),
    }
}

fn open(dir: &Path) -> io::Result<std::path::PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("swoop.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX) {
        let _ = std::fs::rename(&path, dir.join("swoop.old.log"));
    }
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    *FILE.lock().expect("log") = Some(file);
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_grande_vira_old_e_recomeca() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("swoop.log"), vec![b'x'; (MAX + 1) as usize]).unwrap();
        let path = open(dir.path()).unwrap();
        Tee.write_all(b"linha\n").unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "linha\n");
        assert!(dir.path().join("swoop.old.log").exists());
        *FILE.lock().unwrap() = None;
    }
}
