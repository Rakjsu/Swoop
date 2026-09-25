//! Ações da interface sobre a fila: remover (inclusive no meio do download)
//! e pausar/retomar tudo.

mod common;

use common::*;
use std::time::Duration;
use swoop_core::DownloadState;
use swoop_engine::EngineEvent;
use swoop_store::{downloads, history};
use swoop_testsrv::TestServer;

/// Arquivo lento o bastante para ainda estar baixando quando o teste age.
fn slow_file(srv: &TestServer, name: &str) -> String {
    srv.file_url(name, &format!("size={}&seed=3&rate={}", 64 * MIB, MIB))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn remover_no_meio_apaga_o_part_e_libera_a_fila() {
    let srv = TestServer::start().await.unwrap();
    let mut s = settings(2);
    s.max_active_downloads = 1;
    let h = Harness::new(s).await;
    let mut events = h.engine.events();
    let id = h.add(slow_file(&srv, "remover.bin")).await;
    // espera na fila: só anda quando a vaga do removido for liberada
    let next = h
        .add(srv.file_url("depois.bin", &format!("size={}&seed=4", 2 * MIB)))
        .await;
    h.wait_done_bytes(id, 1, Duration::from_secs(20)).await;
    assert_eq!(h.row(next).await.state, DownloadState::Queued);
    let part = h.row(id).await.part_path.expect("sem .part");
    assert!(part.exists());

    h.engine.remove(id, false).await.unwrap();

    let gone = h.store.call(move |c| downloads::get(c, id)).await.unwrap();
    assert!(gone.is_none(), "a linha continua no banco");
    assert!(!part.exists(), "o .part ficou para trás");
    assert_eq!(h.store.call(|c| history::count(c)).await.unwrap(), 1);
    let mut changed = false;
    while let Ok(ev) = events.try_recv() {
        changed |= ev == EngineEvent::Changed;
    }
    assert!(changed, "a interface não foi avisada");

    let rows = h.wait_final(&[next], Duration::from_secs(30)).await;
    completed(&rows[0]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn remover_concluido_so_apaga_o_arquivo_se_pedido() {
    let srv = TestServer::start().await.unwrap();
    let h = Harness::new(settings(2)).await;
    let url = |n: &str| srv.file_url(n, &format!("size={}&seed=5", MIB));
    let keep = h.add(url("fica.bin")).await;
    let del = h.add(url("some.bin")).await;
    let rows = h.wait_final(&[keep, del], Duration::from_secs(30)).await;
    let (keep_path, del_path) = (completed(&rows[0]), completed(&rows[1]));

    h.engine.remove(keep, false).await.unwrap();
    h.engine.remove(del, true).await.unwrap();
    assert!(keep_path.exists(), "apagou o arquivo sem pedirem");
    assert!(!del_path.exists(), "não apagou o arquivo pedido");
    // concluídos já estavam no histórico: remover não duplica
    assert_eq!(h.store.call(|c| history::count(c)).await.unwrap(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pausar_e_retomar_tudo() {
    let srv = TestServer::start().await.unwrap();
    let mut s = settings(2);
    s.max_active_downloads = 1;
    let h = Harness::new(s).await;
    let a = h.add(slow_file(&srv, "a.bin")).await;
    let b = h.add(slow_file(&srv, "b.bin")).await;
    h.wait_done_bytes(a, 1, Duration::from_secs(20)).await;

    h.engine.pause_all().await.unwrap();
    for id in [a, b] {
        assert_eq!(h.row(id).await.state, DownloadState::Paused);
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(h.engine.active_count(), 0, "um job continuou rodando");

    h.engine.resume_all().await.unwrap();
    for id in [a, b] {
        let st = h.row(id).await.state;
        assert!(st != DownloadState::Paused, "{id} continuou pausado");
    }
    for id in [a, b] {
        h.engine.remove(id, false).await.unwrap();
    }
}
