//! Portão 3c, parte do motor: baixar de servidores reais pelos plugins
//! (rede; `#[ignore]`). Links trocáveis por `SWOOP_TEST_*`, como em
//! `crates/hosts/tests/network.rs`.
//!
//! `cargo test -p swoop-engine --test network -- --ignored --test-threads=1 --nocapture`

mod common;

use common::*;
use md5::{Digest, Md5};
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use swoop_hosts::{Registry, Rules};

fn env_or(var: &str, default: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| default.to_owned())
}

async fn harness() -> Harness {
    let registry = Registry::new(Rules::embedded()).unwrap();
    Harness::with_resolver(settings(4), Arc::new(registry)).await
}

fn md5_file(path: &Path) -> String {
    let mut file = std::fs::File::open(path).unwrap();
    let mut hasher = Md5::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf).unwrap();
        if n == 0 {
            return hex::encode(hasher.finalize());
        }
        hasher.update(&buf[..n]);
    }
}

/// Mediafire: o sha256 da API é conferido pelo motor ao terminar.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "rede"]
async fn mediafire_baixa_e_confere_o_sha256() {
    let h = harness().await;
    let link = env_or(
        "SWOOP_TEST_MEDIAFIRE",
        "https://www.mediafire.com/file/ipnyzofjcwri357/test-10mb.bin/file",
    );
    let id = h.add(link).await;
    let row = &h.wait_final(&[id], Duration::from_secs(600)).await[0];
    let path = completed(row);
    println!("mediafire: {} ({:?} bytes)", path.display(), row.size);
    // O plugin passa o sha256 da API (conferido em `crates/hosts/tests/network.rs`);
    // concluído = o motor conferiu e bateu.
    assert_eq!(row.host_key.as_deref(), Some("mediafire"));
}

/// Drive > 100 MB: fecha o motor no meio, reabre e termina com o md5 certo.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "rede"]
async fn drive_grande_fecha_reabre_e_confere_o_md5() {
    let mut h = harness().await;
    let link = env_or(
        "SWOOP_TEST_GDRIVE_BIG",
        "https://drive.google.com/uc?id=1s52ek_4YTDRt_EOkx1FS53u-vJa0c4nu",
    );
    let md5 = env_or(
        "SWOOP_TEST_GDRIVE_BIG_MD5",
        "d221e4cefe458b4820002d782ae1f9a4",
    );
    let id = h.add(link).await;
    h.wait_done_bytes(id, 20 * MIB, Duration::from_secs(300))
        .await;
    let before = h.row(id).await.done_bytes;
    h.reopen(settings(4)).await;
    let row = &h.wait_final(&[id], Duration::from_secs(3600)).await[0];
    let path = completed(row);
    println!("drive: retomou de {before} bytes; {}", path.display());
    assert_eq!(md5_file(&path), md5);
}
