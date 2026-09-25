//! Download do instalador: hash certo grava, hash errado não deixa arquivo.

use swoop_testsrv::{TestServer, effective_seed, sha256_hex};
use swoop_update::{UpdateError, download::fetch_verified};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grava_so_com_hash_certo() {
    let srv = TestServer::start().await.unwrap();
    let client = swoop_net::download_client("teste").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let size = 300_000;
    let url = srv.file_url("setup.exe", &format!("size={size}&seed=21"));
    let good = sha256_hex(effective_seed(21, 0), size);

    let seen = std::sync::atomic::AtomicU64::new(0);
    let path = fetch_verified(&client, &url, "setup.exe", &good, dir.path(), |r, _| {
        seen.store(r, std::sync::atomic::Ordering::Relaxed)
    })
    .await
    .unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().len(), size);
    assert_eq!(seen.load(std::sync::atomic::Ordering::Relaxed), size);

    let bad = "0".repeat(64);
    let err = fetch_verified(&client, &url, "outro.exe", &bad, dir.path(), |_, _| {})
        .await
        .unwrap_err();
    assert_eq!(err, UpdateError::HashMismatch);
    assert!(!dir.path().join("outro.exe").exists());
    assert!(!dir.path().join("outro.exe.part").exists());
}
