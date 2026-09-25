//! Disco no Linux/macOS: `fallocate` para reservar e `pwrite` posicional.

use std::fs::File;
use std::io;
use std::os::unix::fs::FileExt;

/// Reserva `size` bytes (sem gravar zeros); se o sistema de arquivos não
/// suportar, só ajusta o tamanho.
pub fn reserve(file: &File, size: u64) -> io::Result<()> {
    if fs4::FileExt::allocate(file, size).is_err() {
        file.set_len(size)?;
    }
    if file.metadata()?.len() != size {
        file.set_len(size)?;
    }
    Ok(())
}

/// `pwrite` até gravar tudo.
pub fn write_all_at(file: &File, buf: &[u8], offset: u64) -> io::Result<()> {
    file.write_all_at(buf, offset)
}
