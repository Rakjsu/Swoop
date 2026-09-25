//! Portão 1e: 1 de 8 conexões estrangulada → tempo ≤ 1,3× o normal (prova a
//! divisão dinâmica).
//!
//! Fica num binário de teste só dele: mede tempo de relógio, e os testes do
//! mesmo binário rodam em paralelo (o servidor de teste gera conteúdo e
//! disputaria a CPU). O arquivo é grande o bastante para a redistribuição da
//! conexão lenta, que tem um custo quase fixo, não dominar a comparação.
//! Se falhar, a mensagem traz a linha do tempo vista pelo servidor (conexões
//! abertas e bytes enviados a cada 250 ms) para separar "não dividiu" de
//! "máquina lenta".

mod common;

use common::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use swoop_engine::segments::plan;
use swoop_store::DownloadRow;
use swoop_testsrv::{TestServer, effective_seed, sha256_hex};

/// Baixa `url` do começo ao fim, anotando o que o servidor vê. Devolve o
/// arnês junto: a pasta temporária com o arquivo vive enquanto ele viver.
async fn timed(srv: &TestServer, url: String) -> (Harness, DownloadRow, Duration, String) {
    let h = Harness::new(settings(8)).await;
    let done = AtomicBool::new(false);
    let t = Instant::now();
    let download = async {
        let id = h.add(url).await;
        let row = h
            .wait_final(&[id], Duration::from_secs(120))
            .await
            .remove(0);
        done.store(true, Ordering::SeqCst);
        row
    };
    let sampler = async {
        let mut line = Vec::new();
        while !done.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let s = srv.stats();
            line.push(format!(
                "{:.2}s:{}c/{:.0}M",
                t.elapsed().as_secs_f64(),
                s.active_connections,
                s.bytes_sent as f64 / MIB as f64
            ));
        }
        line.join(" ")
    };
    let (row, line) = tokio::join!(download, sampler);
    let elapsed = t.elapsed();
    let requests = srv.stats().requests;
    srv.reset_stats();
    (h, row, elapsed, format!("{requests} requisições; {line}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn conexao_lenta_nao_segura_o_download() {
    let srv = TestServer::start().await.unwrap();
    let size = 128 * MIB;
    let base = format!("size={size}&seed=8&rate={}", 4 * MIB);

    let (_h, row, normal, normal_line) = timed(&srv, srv.file_url("normal.bin", &base)).await;
    completed(&row);

    let slow_at = plan(size, 8)[3].start;
    let url = srv.file_url(
        "lento.bin",
        &format!("{base}&slow_at={slow_at}&slow_rate=65536"),
    );
    let (_h, row, slow, slow_line) = timed(&srv, url).await;
    assert_eq!(
        sha256_file(&completed(&row)),
        sha256_hex(effective_seed(8, 0), size)
    );
    let ratio = slow.as_secs_f64() / normal.as_secs_f64();
    println!("portão 1e: normal {normal:?}, com conexão lenta {slow:?} ({ratio:.2}×)");
    println!("normal: {normal_line}\nlenta: {slow_line}");
    assert!(
        ratio <= 1.3,
        "normal {normal:?}, com conexão lenta {slow:?} ({ratio:.2}×)\nnormal: {normal_line}\nlenta: {slow_line}"
    );
}
