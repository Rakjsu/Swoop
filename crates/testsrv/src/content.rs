//! Conteúdo pseudoaleatório determinístico: o byte na posição `i` depende só
//! de `(seed, i)`. Qualquer faixa é gerada sem disco e o sha256 esperado é
//! calculável pelo teste.

use sha2::{Digest, Sha256};

/// Mistura de 64 bits (splitmix64): rápida e bem distribuída.
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// Semente efetiva de uma "geração" do arquivo (mudar a geração = arquivo novo).
pub fn effective_seed(seed: u64, generation: u64) -> u64 {
    seed ^ splitmix64(generation.wrapping_add(0xA5A5))
}

/// Preenche `buf` com os bytes a partir de `offset`.
pub fn fill(seed: u64, offset: u64, buf: &mut [u8]) {
    let mut pos = offset;
    let mut i = 0;
    while i < buf.len() {
        let block = splitmix64(seed ^ (pos / 8)).to_le_bytes();
        let start = (pos % 8) as usize;
        let n = (8 - start).min(buf.len() - i);
        buf[i..i + n].copy_from_slice(&block[start..start + n]);
        i += n;
        pos += n as u64;
    }
}

/// sha256 (hex) do arquivo inteiro de `size` bytes.
pub fn sha256_hex(seed: u64, size: u64) -> String {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut offset = 0;
    while offset < size {
        let n = (size - offset).min(buf.len() as u64) as usize;
        fill(seed, offset, &mut buf[..n]);
        hasher.update(&buf[..n]);
        offset += n as u64;
    }
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faixas_batem_com_o_todo() {
        let mut all = vec![0u8; 1000];
        fill(7, 0, &mut all);
        for (off, len) in [(0usize, 1usize), (3, 17), (8, 8), (999, 1), (123, 500)] {
            let mut part = vec![0u8; len];
            fill(7, off as u64, &mut part);
            assert_eq!(part, all[off..off + len]);
        }
    }

    #[test]
    fn geracao_muda_o_conteudo() {
        assert_ne!(
            sha256_hex(effective_seed(1, 0), 4096),
            sha256_hex(effective_seed(1, 1), 4096)
        );
    }
}
