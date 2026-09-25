//! Conferência de integridade no fim (leitura sequencial do arquivo inteiro,
//! porque a gravação segmentada é fora de ordem).

use md5::Md5;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use swoop_core::Integrity;

/// Calcula o hash pedido e compara com o anunciado (hex, sem diferenciar caixa).
pub fn matches(path: &Path, integrity: &Integrity) -> io::Result<bool> {
    let (actual, expected) = match integrity {
        Integrity::Sha256(hex) => (hash_file::<Sha256>(path)?, hex),
        Integrity::Md5(hex) => (hash_file::<Md5>(path)?, hex),
    };
    Ok(actual.eq_ignore_ascii_case(expected.trim()))
}

/// Hash hex de um arquivo, lido em blocos de 1 MiB.
fn hash_file<D: Digest>(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = D::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confere_sha256_e_md5() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x");
        std::fs::write(&p, b"abc").unwrap();
        let sha = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(matches(&p, &Integrity::Sha256(sha.to_uppercase())).unwrap());
        assert!(
            matches(
                &p,
                &Integrity::Md5("900150983cd24fb0d6963f7d28e17f72".into())
            )
            .unwrap()
        );
        assert!(!matches(&p, &Integrity::Md5("0".repeat(32))).unwrap());
    }
}
