//! Portão 1e: 1 de 8 conexões estrangulada → tempo ≤ 1,3× o normal (prova a
//! divisão dinâmica).
//!
//! Fica num binário de teste só dele: mede tempo de relógio, e os testes do
//! mesmo binário rodam em paralelo (o servidor de teste gera conteúdo e
//! disputaria a CPU). O arquivo é grande o bastante para a redistribuição da
//! conexão lenta, que tem um custo quase fixo, não dominar a comparação.

mod common;

use common::*;
use std::time::{Duration, Instant};
use swoop_engine::segments::plan;
use swoop_testsrv::{TestServer, effective_seed, sha256_hex};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn conexao_lenta_nao_segura_o_download() {
    let srv = TestServer::start().await.unwrap();
    let size = 128 * MIB;
    let base = format!("size={size}&seed=8&rate={}", 4 * MIB);

    let h = Harness::new(settings(8)).await;
    let t = Instant::now();
    let id = h.add(srv.file_url("normal.bin", &base)).await;
    completed(&h.wait_final(&[id], Duration::from_secs(120)).await[0]);
    let normal = t.elapsed();

    let slow_at = plan(size, 8)[3].start;
    let h = Harness::new(settings(8)).await;
    let t = Instant::now();
    let url = srv.file_url(
        "lento.bin",
        &format!("{base}&slow_at={slow_at}&slow_rate=65536"),
    );
    let id = h.add(url).await;
    let row = &h.wait_final(&[id], Duration::from_secs(120)).await[0];
    let slow = t.elapsed();
    assert_eq!(
        sha256_file(&completed(row)),
        sha256_hex(effective_seed(8, 0), size)
    );
    let ratio = slow.as_secs_f64() / normal.as_secs_f64();
    println!("portão 1e: normal {normal:?}, com conexão lenta {slow:?} ({ratio:.2}×)");
    assert!(
        ratio <= 1.3,
        "normal {normal:?}, com conexão lenta {slow:?} ({ratio:.2}×)"
    );
}
