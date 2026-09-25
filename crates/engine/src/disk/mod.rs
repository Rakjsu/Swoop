//! Arquivo `.part`: criação, reserva de espaço, trava, escrita posicional e
//! renomeação final. Nada aqui usa o cursor do arquivo (seek + write), porque
//! várias faixas são gravadas fora de ordem.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as os;
#[cfg(windows)]
use windows as os;

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Folga exigida além do tamanho do arquivo ao checar espaço livre.
const SPACE_MARGIN: u64 = 64 * 1024 * 1024;

/// Abre (ou cria) o `.part` com trava exclusiva. `fresh` zera o conteúdo;
/// com `size` conhecido o espaço é reservado (esparso no Windows).
pub fn open_part(path: &Path, size: Option<u64>, fresh: bool) -> io::Result<File> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock().map_err(|_| {
        io::Error::new(
            io::ErrorKind::ResourceBusy,
            format!("{} está em uso por outro processo", path.display()),
        )
    })?;
    if fresh {
        file.set_len(0)?;
    }
    if let Some(size) = size {
        os::reserve(&file, size)?;
    }
    Ok(file)
}

/// Garante espaço livre para `needed` bytes (mais uma folga) na pasta.
pub fn check_space(dir: &Path, needed: u64) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let available = fs4::available_space(dir)?;
    if available < needed.saturating_add(SPACE_MARGIN) {
        return Err(io::Error::new(
            io::ErrorKind::StorageFull,
            format!(
                "espaço insuficiente em {}: faltam {}",
                dir.display(),
                swoop_core::units::format_bytes(needed + SPACE_MARGIN - available)
            ),
        ));
    }
    Ok(())
}

/// Grava `buf` inteiro na posição `offset`.
pub fn write_all_at(file: &File, buf: &[u8], offset: u64) -> io::Result<()> {
    os::write_all_at(file, buf, offset)
}

/// Caminho livre na pasta: `nome.ext`, `nome (1).ext`, `nome (2).ext`…
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (1..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// Renomeia o `.part` para o nome final, tentando por até 5 s (antivírus e
/// indexadores do Windows seguram o arquivo logo depois de fechado).
pub fn finalize(part: &Path, target: &Path) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match std::fs::rename(part, target) {
            Ok(()) => return Ok(()),
            Err(e) if Instant::now() < deadline && e.kind() != io::ErrorKind::NotFound => {
                std::thread::sleep(Duration::from_millis(250));
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escrita_fora_de_ordem_e_nome_unico() {
        let dir = tempfile::tempdir().unwrap();
        let part = dir.path().join("a.bin.part");
        let file = open_part(&part, Some(10), true).unwrap();
        write_all_at(&file, b"56789", 5).unwrap();
        write_all_at(&file, b"01234", 0).unwrap();
        drop(file);
        assert_eq!(std::fs::read(&part).unwrap(), b"0123456789");

        std::fs::write(dir.path().join("a.bin"), b"x").unwrap();
        let target = unique_path(dir.path(), "a.bin");
        assert_eq!(target, dir.path().join("a (1).bin"));
        finalize(&part, &target).unwrap();
        assert!(target.exists() && !part.exists());
    }

    #[test]
    fn trava_impede_segundo_escritor() {
        let dir = tempfile::tempdir().unwrap();
        let part = dir.path().join("b.part");
        let _first = open_part(&part, None, true).unwrap();
        assert!(open_part(&part, None, false).is_err());
    }
}
