//! Portões da fase 1 que rodam dentro do processo (b, c, d, e, f) e o básico:
//! baixar com várias conexões, pausar e retomar.

mod common;

use common::*;
use std::time::{Duration, Instant};
use swoop_core::DownloadState;
use swoop_engine::segments::plan;
use swoop_testsrv::{TestServer, effective_seed, sha256_hex};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn baixa_com_oito_conexoes_e_hash_confere() {
    let srv = TestServer::start().await.unwrap();
    let h = Harness::new(settings(8)).await;
    let size = 32 * MIB;
    // Taxa por conexão: as 8 ficam abertas juntas mesmo com a CPU disputada.
    let url = srv.file_url("oito.bin", &format!("size={size}&seed=3&rate={}", 2 * MIB));
    let id = h.add(url).await;

    let row = &h.wait_final(&[id], Duration::from_secs(60)).await[0];
    let path = completed(row);
    assert_eq!(path.file_name().unwrap(), "oito.bin");
    assert_eq!(sha256_file(&path), sha256_hex(effective_seed(3, 0), size));
    assert_eq!(srv.stats().peak_connections, 8);
    assert!(!h.dest().join("oito.bin.part").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pausa_e_retoma_sem_recomecar() {
    let srv = TestServer::start().await.unwrap();
    let h = Harness::new(settings(4)).await;
    let size = 24 * MIB;
    let url = srv.file_url("pausa.bin", &format!("size={size}&seed=4&rate={}", 2 * MIB));
    let id = h.add(url).await;

    h.wait_done_bytes(id, 4 * MIB, Duration::from_secs(20))
        .await;
    h.engine.pause(id).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let paused = h.row(id).await;
    assert_eq!(paused.state, DownloadState::Paused);
    let sent_before = srv.stats().bytes_sent;

    h.engine.resume(id).await.unwrap();
    let row = &h.wait_final(&[id], Duration::from_secs(60)).await[0];
    assert_eq!(
        sha256_file(&completed(row)),
        sha256_hex(effective_seed(4, 0), size)
    );
    // Retomou do ponto: o total enviado não chega a duas vezes o arquivo.
    assert!(
        srv.stats().bytes_sent < size + sent_before,
        "baixou de novo o que já tinha"
    );
}

/// Portão 1c: servidor sem Range → uma conexão, sem retomada, termina certo.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn servidor_sem_range_usa_uma_conexao() {
    let srv = TestServer::start().await.unwrap();
    let h = Harness::new(settings(8)).await;
    let size = 8 * MIB;
    let id = h
        .add(srv.file_url("inteiro.bin", &format!("size={size}&seed=5&norange=1")))
        .await;

    let row = &h.wait_final(&[id], Duration::from_secs(60)).await[0];
    assert_eq!(
        sha256_file(&completed(row)),
        sha256_hex(effective_seed(5, 0), size)
    );
    assert!(!row.resumable);
    // Sonda + uma única requisição do arquivo.
    assert_eq!(srv.stats().requests, 2);
}

/// Portão 1d: ETag trocado entre sessões → detecta e recomeça do zero.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn arquivo_mudado_entre_sessoes_recomeca() {
    let srv = TestServer::start().await.unwrap();
    let mut h = Harness::new(settings(4)).await;
    let size = 16 * MIB;
    let url = srv.file_url("muda.bin", &format!("size={size}&seed=6&rate={}", 2 * MIB));
    let id = h.add(url).await;
    h.wait_done_bytes(id, 3 * MIB, Duration::from_secs(20))
        .await;

    srv.set_generation(1);
    h.reopen(settings(4)).await;
    let row = &h.wait_final(&[id], Duration::from_secs(60)).await[0];
    assert_eq!(
        sha256_file(&completed(row)),
        sha256_hex(effective_seed(6, 1), size)
    );
}

/// Portão 1b: limite de 2 MiB/s medido em 10 s; trocado para 5 MiB/s na hora.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn limite_de_velocidade_global() {
    let srv = TestServer::start().await.unwrap();
    let mut s = settings(4);
    s.speed_limit_bps = Some(2 * MIB);
    let h = Harness::new(s).await;
    let id = h
        .add(srv.file_url("limite.bin", &format!("size={}&seed=7", 200 * MIB)))
        .await;
    let mut snaps = h.engine.snapshots();
    let received = |snaps: &tokio::sync::watch::Receiver<swoop_engine::Snapshot>| {
        snaps
            .borrow()
            .active
            .iter()
            .find(|r| r.id == id.0)
            .map_or(0, |r| r.received)
    };

    let t0 = Instant::now();
    tokio::time::sleep(Duration::from_secs(3)).await;
    snaps.changed().await.unwrap();
    let (a, ta) = (received(&snaps), Instant::now());
    tokio::time::sleep(Duration::from_secs(10)).await;
    snaps.changed().await.unwrap();
    let (b, tb) = (received(&snaps), Instant::now());
    let rate = (b - a) as f64 / (tb - ta).as_secs_f64() / MIB as f64;
    println!("portão 1b: limite 2 MiB/s medido {rate:.3} MiB/s");
    assert!(
        (1.8..=2.2).contains(&rate),
        "2 MiB/s medido como {rate:.2} MiB/s"
    );

    h.engine.set_speed_limit(Some(5 * MIB));
    tokio::time::sleep(Duration::from_secs(2)).await;
    snaps.changed().await.unwrap();
    let (c, tc) = (received(&snaps), Instant::now());
    tokio::time::sleep(Duration::from_secs(5)).await;
    snaps.changed().await.unwrap();
    let (d, td) = (received(&snaps), Instant::now());
    let rate = (d - c) as f64 / (td - tc).as_secs_f64() / MIB as f64;
    println!("portão 1b: limite 5 MiB/s medido {rate:.3} MiB/s");
    assert!(
        (4.5..=5.5).contains(&rate),
        "5 MiB/s medido como {rate:.2} MiB/s"
    );
    assert!(t0.elapsed() < Duration::from_secs(40));
    h.engine.pause(id).await.unwrap();
}

/// Portão 1e: 1 de 8 conexões estrangulada → tempo ≤ 1,3× o normal.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn conexao_lenta_nao_segura_o_download() {
    let srv = TestServer::start().await.unwrap();
    let size = 64 * MIB;
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

/// Portão 1f: 20 na fila respeitam simultâneos e conexões por servidor.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fila_respeita_limites() {
    let srv = TestServer::start().await.unwrap();
    let mut s = settings(2);
    s.max_active_downloads = 3;
    s.max_connections_per_host = 16;
    let h = Harness::new(s.clone()).await;
    let query = format!("size={}&seed=9&rate={}", 4 * MIB, 4 * MIB);
    let mut ids = Vec::new();
    for i in 0..20 {
        ids.push(h.add(srv.file_url(&format!("f{i}.bin"), &query)).await);
    }
    for row in h.wait_final(&ids, Duration::from_secs(120)).await {
        completed(&row);
    }
    let stats = srv.stats();
    println!("portão 1f: {stats:?}");
    assert_eq!(stats.peak_downloads, 3, "{stats:?}");
    assert!(stats.peak_connections <= 6, "{stats:?}");

    // Agora o limite por servidor manda: 4 conexões no total.
    srv.reset_stats();
    s.connections_per_download = 8;
    s.max_connections_per_host = 4;
    h.engine.update_settings(s);
    let mut ids = Vec::new();
    for i in 0..6 {
        ids.push(h.add(srv.file_url(&format!("g{i}.bin"), &query)).await);
    }
    for row in h.wait_final(&ids, Duration::from_secs(120)).await {
        completed(&row);
    }
    assert!(srv.stats().peak_connections <= 4, "{:?}", srv.stats());
}
