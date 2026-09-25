use super::*;
use crate::ServiceOptions;
use std::time::Duration;
use swoop_api::Push;
use swoop_core::{DownloadState, PackageId, Settings};

/// Serviço numa pasta temporária (banco e downloads dentro dela).
async fn open(dir: &tempfile::TempDir) -> Service {
    Service::open(ServiceOptions {
        data_dir: Some(dir.path().join("dados")),
        settings: Some(Settings {
            download_dir: Some(dir.path().join("baixados")),
            ..Settings::default()
        }),
    })
    .await
    .unwrap()
}

/// Espera um `Push::Changed` (ignora o resto) por até 5 s.
async fn changed(sub: &mut Subscription) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while sub.pushes.recv().await.unwrap() != Push::Changed {}
    })
    .await
    .expect("nenhum aviso de mudança");
}

#[tokio::test(flavor = "multi_thread")]
async fn link_invalido_vira_erro_para_o_usuario() {
    let dir = tempfile::tempdir().unwrap();
    let svc = open(&dir).await;
    let err = svc
        .exec(Command::AddLinks {
            links: vec!["isso não é link".into()],
            dest: None,
        })
        .await
        .unwrap_err();
    assert!(matches!(&err, ApiError::Invalid(m) if m.contains("link inválido")));
    let err = svc.exec(Command::Pause { id: 999 }).await.unwrap_err();
    assert!(matches!(err, ApiError::Invalid(_)));
    svc.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn adicionar_pausar_e_remover_avisam_a_interface() {
    let dir = tempfile::tempdir().unwrap();
    let svc = open(&dir).await;
    let mut sub = svc.subscribe();

    // porta 9 recusa a conexão: o download fica tentando, sem rede externa
    let reply = svc
        .exec(Command::AddLinks {
            links: vec!["http://127.0.0.1:9/a.bin".into()],
            dest: Some("  ".into()),
        })
        .await
        .unwrap();
    let Reply::Added { ids } = reply else {
        panic!("resposta inesperada: {reply:?}");
    };
    assert_eq!(ids.len(), 1);
    changed(&mut sub).await;

    svc.exec(Command::PauseAll).await.unwrap();
    let rows = svc.list().await.unwrap();
    assert_eq!(rows[0].state, DownloadState::Paused);
    assert_eq!(rows[0].url, "http://127.0.0.1:9/a.bin");

    svc.exec(Command::Remove {
        id: ids[0],
        delete_file: false,
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !svc.list().await.unwrap().is_empty() {
            changed(&mut sub).await;
        }
    })
    .await
    .expect("o download não saiu da lista");
    svc.shutdown().await;
    svc.shutdown().await; // idempotente
}

#[test]
fn linha_do_banco_vira_linha_da_interface() {
    let row = DownloadRow {
        id: DownloadId(4),
        package_id: PackageId(1),
        url: "http://x/a.bin".into(),
        host_key: Some("x".into()),
        state: DownloadState::Completed,
        wait_until: None,
        wait_reason: None,
        error_kind: None,
        error_msg: None,
        attempts: 0,
        file_name: Some("a.bin".into()),
        size: Some(10),
        done_bytes: 10,
        resumable: true,
        etag: None,
        last_modified: None,
        part_path: None,
        final_path: Some(PathBuf::from("/tmp/a.bin")),
        position: 1,
        started_at: None,
        finished_at: None,
    };
    let v = view(row);
    assert_eq!(v.id, 4);
    assert_eq!(v.file_name.as_deref(), Some("a.bin"));
    assert_eq!(v.final_path.as_deref(), Some("/tmp/a.bin"));
    assert_eq!(v.state, DownloadState::Completed);
}
