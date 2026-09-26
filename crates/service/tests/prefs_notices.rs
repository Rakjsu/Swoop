//! Preferências salvas, pasta automática por tipo, histórico e avisos da
//! fila, pelo mesmo `Backend` que as interfaces usam.

use std::path::Path;
use std::time::Duration;
use swoop_api::{Backend, Command, HistoryOutcome, Notice, Push, Settings, Subscription};
use swoop_service::{Service, ServiceOptions};
use swoop_testsrv::TestServer;

const MIB: u64 = 1024 * 1024;

/// Serviço com as preferências salvas no banco da pasta temporária.
async fn open_saved(dir: &Path) -> Service {
    Service::open(ServiceOptions {
        data_dir: Some(dir.join("dados")),
        settings: None,
    })
    .await
    .unwrap()
}

/// Próximo aviso (`Push::Notice`), ignorando retratos e mudanças.
async fn next_notice(sub: &mut Subscription) -> Notice {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Some(Push::Notice(n)) = sub.next().await {
                return n;
            }
        }
    })
    .await
    .expect("nenhum aviso chegou")
}

#[tokio::test(flavor = "multi_thread")]
async fn preferencias_salvas_voltam_ao_reabrir() {
    let dir = tempfile::tempdir().unwrap();
    let svc = open_saved(dir.path()).await;
    let mut s = Backend::settings(&svc).await.unwrap();
    assert!(
        s.download_dir.is_some(),
        "pasta de downloads não preenchida"
    );
    s.max_active_downloads = 2;
    s.speed_limit_bps = Some(MIB);
    s.video_dir = Some(dir.path().join("videos"));
    s.notify = false;
    svc.exec(Command::SaveSettings {
        settings: s.clone(),
    })
    .await
    .unwrap();
    let bad = Settings {
        music_dir: Some("relativa".into()),
        ..s.clone()
    };
    assert!(
        svc.exec(Command::SaveSettings { settings: bad })
            .await
            .is_err()
    );
    svc.shutdown().await;
    drop(svc);

    let svc = open_saved(dir.path()).await;
    assert_eq!(Backend::settings(&svc).await.unwrap(), s);
    // o limite mudado pela barra também fica salvo
    svc.exec(Command::SetSpeedLimit { bps: None })
        .await
        .unwrap();
    svc.shutdown().await;
    drop(svc);
    let svc = open_saved(dir.path()).await;
    assert_eq!(svc.settings().speed_limit_bps, None);
    svc.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn pasta_por_tipo_historico_e_avisos() {
    let srv = TestServer::start().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let svc = Service::open(ServiceOptions {
        data_dir: Some(dir.path().join("dados")),
        settings: Some(Settings {
            download_dir: Some(dir.path().join("downloads")),
            video_dir: Some(dir.path().join("videos")),
            music_dir: Some(dir.path().join("musicas")),
            max_retries: 0,
            ..Settings::default()
        }),
    })
    .await
    .unwrap();
    let mut sub = svc.subscribe();
    let q = format!("size={MIB}&seed=2");
    let links = vec![
        srv.file_url("aula.mp4", &q),
        srv.file_url("faixa.flac", &q),
        srv.file_url("pacote.zip", &q),
        "http://127.0.0.1:9/offline.bin".to_owned(),
    ];
    svc.exec(Command::AddLinks { links, dest: None })
        .await
        .unwrap();

    let mut failed = 0;
    let finished = loop {
        match next_notice(&mut sub).await {
            Notice::Failed { name, .. } => {
                assert_eq!(name, "offline.bin");
                failed += 1;
            }
            done @ Notice::QueueFinished { .. } => break done,
            other => panic!("aviso inesperado: {other:?}"),
        }
    };
    assert_eq!(failed, 1);
    assert_eq!(
        finished,
        Notice::QueueFinished {
            completed: 3,
            failed: 1
        }
    );
    for (sub_dir, name) in [
        ("videos", "aula.mp4"),
        ("musicas", "faixa.flac"),
        ("downloads", "pacote.zip"),
    ] {
        let path = dir.path().join(sub_dir).join(name);
        assert!(path.exists(), "{name} não foi para {sub_dir}");
    }

    let history = Backend::history(&svc, 10).await.unwrap();
    assert_eq!(history.len(), 4);
    let outcomes = |o| history.iter().filter(|h| h.outcome == o).count();
    assert_eq!(outcomes(HistoryOutcome::Completed), 3);
    assert_eq!(outcomes(HistoryOutcome::Failed), 1);
    svc.shutdown().await;
}
