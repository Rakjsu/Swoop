//! Disco no Windows.
//!
//! O NTFS preenche com zeros tudo entre o fim válido do arquivo e uma escrita
//! feita mais à frente; com 8 faixas num arquivo de vários GB a primeira
//! escrita da última faixa travaria segundos. Marcando o `.part` como esparso
//! (`FSCTL_SET_SPARSE`) as regiões não gravadas não são zeradas.

use std::fs::File;
use std::io;
use std::os::windows::fs::FileExt;
use std::os::windows::io::AsRawHandle;
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::FSCTL_SET_SPARSE;

/// Marca como esparso (se o volume suportar) e ajusta o tamanho.
pub fn reserve(file: &File, size: u64) -> io::Result<()> {
    set_sparse(file);
    if file.metadata()?.len() != size {
        file.set_len(size)?;
    }
    Ok(())
}

/// Liga o atributo esparso; falha (FAT32, exFAT) é ignorada e o custo de
/// zerar passa a ser aceito.
fn set_sparse(file: &File) {
    let mut returned: u32 = 0;
    // SAFETY: handle válido enquanto `file` vive; sem buffers de entrada/saída.
    let ok = unsafe {
        DeviceIoControl(
            file.as_raw_handle() as _,
            FSCTL_SET_SPARSE,
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        tracing::debug!(
            "volume sem suporte a arquivo esparso: {}",
            io::Error::last_os_error()
        );
    }
}

/// `seek_write` até gravar tudo (só a escritora mexe no cursor do arquivo).
pub fn write_all_at(file: &File, mut buf: &[u8], mut offset: u64) -> io::Result<()> {
    while !buf.is_empty() {
        match file.seek_write(buf, offset) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "o disco não aceitou mais bytes",
                ));
            }
            Ok(n) => {
                buf = &buf[n..];
                offset += n as u64;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
