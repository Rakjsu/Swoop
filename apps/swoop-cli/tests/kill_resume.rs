//! Portão 1a: matar o `swoop-cli` (SIGKILL / TerminateProcess) em pontos
//! aleatórios, 5 vezes seguidas, e retomar; o arquivo final tem o sha256
//! certo e o progresso salvo nunca anda para trás.

use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use swoop_store::{Store, downloads};
use swoop_testsrv::{TestServer, effective_seed, sha256_hex};

const MIB: u64 = 1024 * 1024;

/// Sobe o CLI com a pasta de dados do teste (sem saída no terminal).
fn spawn(args: &[&str]) -> Child {
    Command::new(env!("CARGO_BIN_EXE_swoop-cli"))
        .args(args)
        .env("RUST_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("não subiu o swoop-cli")
}

/// Gerador pseudoaleatório simples (xorshift) semeado pelo relógio.
struct Rng(u64);

impl Rng {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(nanos as u64 | 1)
    }

    /// Duração uniforme em `[min_ms, max_ms)`.
    fn millis(&mut self, min_ms: u64, max_ms: u64) -> Duration {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        Duration::from_millis(min_ms + self.0 % (max_ms - min_ms))
    }
}

/// Bytes confirmados no banco (lido com o CLI parado).
async fn done_bytes(data: &Path) -> u64 {
    let store = Store::open(&data.join("swoop.sqlite")).unwrap();
    let rows = store.call(|c| downloads::list(c)).await.unwrap();
    rows.first().map_or(0, |r| r.done_bytes)
}

/// Espera o processo terminar sozinho (com limite).
async fn wait_exit(child: &mut Child, timeout: Duration) -> std::process::ExitStatus {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        assert!(Instant::now() < deadline, "o download não terminou a tempo");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Roda o portão para um arquivo de `size` bytes a `rate` bytes/s por conexão.
async fn kill_and_resume(size: u64, rate: u64) {
    let srv = TestServer::start().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("dados");
    let dest = dir.path().join("baixados");
    let url = srv.file_url("grande.bin", &format!("size={size}&seed=11&rate={rate}"));
    let (data_s, dest_s) = (data.to_str().unwrap(), dest.to_str().unwrap());

    let mut rng = Rng::new();
    let mut child = spawn(&[
        "get",
        &url,
        "--data-dir",
        data_s,
        "--dest",
        dest_s,
        "--connections",
        "8",
    ]);
    let mut last = 0;
    for round in 1..=5 {
        tokio::time::sleep(rng.millis(1000, 2000)).await;
        assert!(
            child.try_wait().unwrap().is_none(),
            "terminou antes da morte nº {round}: aumente o arquivo ou reduza a taxa"
        );
        child.kill().unwrap();
        child.wait().unwrap();

        let now = done_bytes(&data).await;
        assert!(now >= last, "progresso andou para trás: {last} → {now}");
        println!(
            "morte {round}: {:.1} MiB confirmados",
            now as f64 / MIB as f64
        );
        last = now;
        child = spawn(&["resume", "--data-dir", data_s, "--connections", "8"]);
    }

    let status = wait_exit(&mut child, Duration::from_secs(300)).await;
    assert!(status.success(), "resume terminou com {status}");
    let file = dest.join("grande.bin");
    let hash = hex::encode(Sha256::digest(std::fs::read(&file).unwrap()));
    assert_eq!(hash, sha256_hex(effective_seed(11, 0), size));
    assert!(!dest.join("grande.bin.part").exists());
    assert!(last > 0, "nenhum progresso foi salvo entre as mortes");
    let sent = srv.stats().bytes_sent;
    println!(
        "enviados {:.1} MiB para um arquivo de {} MiB",
        sent as f64 / MIB as f64,
        size / MIB
    );
    assert!(sent < size * 2, "baixou quase tudo de novo: {sent} bytes");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn matar_e_retomar_cinco_vezes_128_mib() {
    kill_and_resume(128 * MIB, 3 * MIB / 2).await;
}

/// Versão de 1 GiB do portão (rodar à mão: `-- --ignored`).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn matar_e_retomar_cinco_vezes_1_gib() {
    kill_and_resume(1024 * MIB, 12 * MIB).await;
}
