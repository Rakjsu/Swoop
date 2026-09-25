//! A instalação em si: grava o setup NSIS embutido numa pasta temporária e o
//! executa em silêncio (`/S /D=<pasta>`); depois abre o Swoop sem privilégio
//! de administrador.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Nome da pasta do app dentro de Arquivos de Programas.
pub const APP_DIR_NAME: &str = "Swoop";
/// Executável principal instalado.
pub const APP_EXE: &str = "swoop.exe";

/// `C:\Program Files\Swoop` (64 bits, mesmo se o processo for de 32).
pub fn default_dir() -> PathBuf {
    let base = std::env::var_os("ProgramW6432")
        .or_else(|| std::env::var_os("ProgramFiles"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
    base.join(APP_DIR_NAME)
}

/// Pasta escolhida pelo usuário: se não termina em `Swoop`, instala numa
/// subpasta `Swoop` (mesmo comportamento do instalador NSIS assistido).
pub fn normalize_dir(picked: &Path) -> PathBuf {
    let already = picked
        .file_name()
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(APP_DIR_NAME));
    if already {
        picked.to_path_buf()
    } else {
        picked.join(APP_DIR_NAME)
    }
}

/// Recusa pastas que o `/D=` do NSIS não aceita.
pub fn validate_dir(dir: &Path) -> Result<(), String> {
    let text = dir.to_string_lossy();
    if !dir.is_absolute() {
        return Err("escolha uma pasta completa (ex.: C:\\Program Files\\Swoop)".into());
    }
    if text.contains('"') {
        return Err("o caminho da pasta não pode ter aspas".into());
    }
    Ok(())
}

/// Grava o setup e o executa em silêncio; devolve o código de saída.
pub fn run(payload: &[u8], version: &str, dir: &Path) -> Result<i32, String> {
    if payload.is_empty() {
        return Err(
            "este instalador foi gerado sem o pacote do Swoop (build de desenvolvimento)".into(),
        );
    }
    validate_dir(dir)?;
    let setup = std::env::temp_dir().join(format!("swoop-setup-{version}.exe"));
    std::fs::write(&setup, payload)
        .map_err(|e| format!("não consegui preparar a instalação: {e}"))?;
    let result = run_setup(&setup, dir);
    let _ = std::fs::remove_file(&setup);
    result
}

/// `/D=` precisa ser o último argumento e sem aspas: tudo depois dele é a pasta.
#[cfg(windows)]
fn run_setup(setup: &Path, dir: &Path) -> Result<i32, String> {
    use std::os::windows::process::CommandExt;
    let status = Command::new(setup)
        .raw_arg(format!("/S /D={}", dir.display()))
        .status()
        .map_err(|e| format!("não consegui iniciar a instalação: {e}"))?;
    Ok(status.code().unwrap_or(-1))
}

#[cfg(not(windows))]
fn run_setup(_setup: &Path, _dir: &Path) -> Result<i32, String> {
    Err("o instalador do Swoop só funciona no Windows".into())
}

/// Abre o Swoop instalado. Pelo `explorer.exe` o Windows inicia o app como o
/// usuário comum, e não herdando o administrador deste instalador.
pub fn launch(dir: &Path) -> Result<(), String> {
    let exe = dir.join(APP_EXE);
    if !exe.exists() {
        return Err(format!("{} não foi encontrado", exe.display()));
    }
    Command::new("explorer.exe")
        .arg(&exe)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("não consegui abrir o Swoop: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subpasta_swoop_quando_falta() {
        assert_eq!(normalize_dir(Path::new("/opt")), Path::new("/opt/Swoop"));
        assert_eq!(
            normalize_dir(Path::new("/opt/swoop")),
            Path::new("/opt/swoop")
        );
    }

    #[test]
    fn recusa_pasta_relativa_ou_com_aspas() {
        assert!(validate_dir(Path::new("relativa")).is_err());
        assert!(validate_dir(Path::new("/com\"aspas")).is_err());
        assert!(validate_dir(&default_dir_for_test()).is_ok());
    }

    #[test]
    fn sem_pacote_nao_instala() {
        let err = run(&[], "0.0.0", &default_dir_for_test()).unwrap_err();
        assert!(err.contains("sem o pacote"));
    }

    fn default_dir_for_test() -> PathBuf {
        std::env::temp_dir().join(APP_DIR_NAME)
    }
}
