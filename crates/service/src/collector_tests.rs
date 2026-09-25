//! Coletor de ponta a ponta com o servidor de teste: colar texto, conferir,
//! iniciar num pacote e "iniciar sozinho".

use crate::{Service, ServiceOptions};
use std::time::{Duration, Instant};
use swoop_api::{Backend, CollectorView, Command, Reply};
use swoop_core::{LinkState, Settings};
use swoop_testsrv::TestServer;

async fn open(dir: &tempfile::TempDir, auto_start: bool) -> Service {
    Service::open(ServiceOptions {
        data_dir: Some(dir.path().join("dados")),
        settings: Some(Settings {
            download_dir: Some(dir.path().join("baixados")),
            auto_start,
            ..Settings::default()
        }),
    })
    .await
    .unwrap()
}

/// Espera o coletor terminar de conferir tudo (até 5 s).
async fn checked(svc: &Service) -> Vec<CollectorView> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let rows = svc.collector().await.unwrap();
        let busy = rows
            .iter()
            .any(|r| matches!(r.state, LinkState::Unchecked | LinkState::Checking));
        if !busy {
            return rows;
        }
        assert!(Instant::now() < deadline, "coletor não terminou: {rows:?}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn cola_confere_e_inicia_num_pacote() {
    let srv = TestServer::start().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let svc = open(&dir, false).await;
    let a = srv.file_url("serie.part1.rar", "size=1000&seed=1");
    let b = srv.file_url("serie.part2.rar", "size=2000&seed=2");
    let gone = a.replace("/file/", "/sumiu/");
    let text = format!("baixe {a}\ne também {b}, mas não {gone}.");

    let t0 = Instant::now();
    let reply = svc
        .exec(Command::Collect { text: text.clone() })
        .await
        .unwrap();
    assert_eq!(reply, Reply::Collected { added: 3 });
    // colar de novo não duplica
    let again = svc.exec(Command::Collect { text }).await.unwrap();
    assert_eq!(again, Reply::Collected { added: 0 });

    let rows = checked(&svc).await;
    assert!(t0.elapsed() <= Duration::from_secs(5));
    let states: Vec<_> = rows.iter().map(|r| r.state).collect();
    assert_eq!(
        states,
        [LinkState::Online, LinkState::Online, LinkState::Offline]
    );
    assert_eq!(rows[0].file_name.as_deref(), Some("serie.part1.rar"));
    assert_eq!(rows[1].size, Some(2000));

    let ids = rows.iter().map(|r| r.id).collect();
    let reply = svc
        .exec(Command::CollectorStart { ids, dest: None })
        .await
        .unwrap();
    let Reply::Added { ids } = reply else {
        panic!("resposta inesperada: {reply:?}");
    };
    assert_eq!(ids.len(), 2, "o offline fica no coletor");
    let left = svc.collector().await.unwrap();
    assert_eq!(left.len(), 1);
    svc.exec(Command::CollectorRemoveOffline).await.unwrap();
    assert!(svc.collector().await.unwrap().is_empty());

    let list = svc.list().await.unwrap();
    assert_eq!(list.len(), 2);
    assert!(list.iter().all(|d| d.package_name == "serie"));
    svc.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn iniciar_sozinho_manda_para_a_fila() {
    let srv = TestServer::start().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let svc = open(&dir, true).await;
    let text = format!(
        "{} {}",
        srv.file_url("a.bin", "size=1000"),
        srv.file_url("b.bin", "size=1000")
    );
    svc.exec(Command::Collect { text }).await.unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    while svc.list().await.unwrap().len() < 2 {
        assert!(Instant::now() < deadline, "não foi para a fila sozinho");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(svc.collector().await.unwrap().is_empty());
    svc.shutdown().await;
}
