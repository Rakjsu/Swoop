use super::*;
use crate::packages;

/// Banco em memória migrado com um pacote.
fn setup() -> (Connection, PackageId) {
    let mut c = Connection::open_in_memory().unwrap();
    crate::migrations::run(&mut c).unwrap();
    let pkg = packages::insert(&c, "p", Path::new("/tmp"), false).unwrap();
    (c, pkg)
}

#[test]
fn insere_na_ordem_da_fila() {
    let (c, pkg) = setup();
    let a = insert(&c, pkg, "http://x/a").unwrap();
    let b = insert(&c, pkg, "http://x/b").unwrap();
    let q = queued(&c, 10).unwrap();
    assert_eq!(q.iter().map(|r| r.id).collect::<Vec<_>>(), vec![a, b]);
    assert_eq!(q[0].state, DownloadState::Queued);
}

#[test]
fn transicao_valida_grava_e_invalida_recusa() {
    let (c, pkg) = setup();
    let id = insert(&c, pkg, "http://x/a").unwrap();
    assert_eq!(
        transition(&c, id, Event::Start).unwrap(),
        DownloadState::Resolving
    );
    assert!(get(&c, id).unwrap().unwrap().started_at.is_some());
    let err = transition(&c, id, Event::Verified).unwrap_err();
    assert!(matches!(err, StoreError::Transition(_)));
    assert_eq!(
        get(&c, id).unwrap().unwrap().state,
        DownloadState::Resolving
    );
}

#[test]
fn espera_vencida_volta_para_fila() {
    let (c, pkg) = setup();
    let id = insert(&c, pkg, "http://x/a").unwrap();
    transition(&c, id, Event::Start).unwrap();
    wait(&c, id, 1_000, WaitReason::Backoff, Some("tentativa 1")).unwrap();
    let row = get(&c, id).unwrap().unwrap();
    assert_eq!(row.state, DownloadState::Waiting);
    assert_eq!(row.wait_reason, Some(WaitReason::Backoff));

    assert_eq!(wake_due(&c, 999).unwrap(), 0);
    assert_eq!(wake_due(&c, 1_000).unwrap(), 1);
    let row = get(&c, id).unwrap().unwrap();
    assert_eq!(row.state, DownloadState::Queued);
    assert_eq!(row.wait_until, None);
}

#[test]
fn falha_grava_motivo_e_nova_tentativa_limpa() {
    let (c, pkg) = setup();
    let id = insert(&c, pkg, "http://x/a").unwrap();
    transition(&c, id, Event::Start).unwrap();
    fail(&c, id, Event::Fail, "http", "HTTP 404").unwrap();
    let row = get(&c, id).unwrap().unwrap();
    assert_eq!(row.state, DownloadState::Failed);
    assert_eq!(row.error_msg.as_deref(), Some("HTTP 404"));
    assert!(row.finished_at.is_some());

    transition(&c, id, Event::Retry).unwrap();
    let row = get(&c, id).unwrap().unwrap();
    assert_eq!(row.state, DownloadState::Queued);
    assert_eq!(row.error_msg, None);
}

#[test]
fn reabrir_recupera_ativos() {
    let (c, pkg) = setup();
    let a = insert(&c, pkg, "http://x/a").unwrap();
    let b = insert(&c, pkg, "http://x/b").unwrap();
    transition(&c, a, Event::Start).unwrap();
    transition(&c, a, Event::Resolved).unwrap();
    transition(&c, b, Event::Pause).unwrap();
    assert_eq!(recover_all(&c).unwrap(), 1);
    assert_eq!(get(&c, a).unwrap().unwrap().state, DownloadState::Queued);
    assert_eq!(get(&c, b).unwrap().unwrap().state, DownloadState::Paused);
}

#[test]
fn sonda_e_caminho_final() {
    let (c, pkg) = setup();
    let id = insert(&c, pkg, "http://x/a").unwrap();
    let probe = ProbeInfo {
        host_key: "x".into(),
        file_name: "a.bin".into(),
        size: Some(123),
        resumable: true,
        etag: Some("\"v1\"".into()),
        last_modified: None,
        part_path: PathBuf::from("/tmp/a.bin.part"),
    };
    set_probe(&c, id, &probe).unwrap();
    let row = get(&c, id).unwrap().unwrap();
    assert_eq!(row.size, Some(123));
    assert_eq!(row.etag.as_deref(), Some("\"v1\""));
    set_final_path(&c, id, Path::new("/tmp/a.bin")).unwrap();
    let row = get(&c, id).unwrap().unwrap();
    assert_eq!(row.final_path, Some(PathBuf::from("/tmp/a.bin")));
    assert_eq!(row.part_path, None);
}

#[test]
fn remover_registra_historico_e_apaga_pacote_vazio() {
    let (mut c, pkg) = setup();
    let a = insert(&c, pkg, "http://x/a").unwrap();
    let b = insert(&c, pkg, "http://x/b").unwrap();
    let seg = crate::SegmentRow {
        idx: 0,
        start: 0,
        end: 10,
        pos: 0,
    };
    crate::segments::replace(&mut c, a, &[seg]).unwrap();

    let row = remove(&mut c, a).unwrap().unwrap();
    assert_eq!(row.id, a);
    assert!(get(&c, a).unwrap().is_none());
    assert!(crate::segments::load(&c, a).unwrap().is_empty());
    assert_eq!(crate::history::count(&c).unwrap(), 1);

    // o pacote continua enquanto tiver downloads
    let pkgs = |c: &Connection| -> i64 {
        c.query_row("SELECT COUNT(*) FROM packages", [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(pkgs(&c), 1);
    remove(&mut c, b).unwrap();
    assert_eq!(pkgs(&c), 0);
    assert!(remove(&mut c, b).unwrap().is_none());
}

#[test]
fn remover_concluido_nao_duplica_historico() {
    let (mut c, pkg) = setup();
    let id = insert(&c, pkg, "http://x/a").unwrap();
    for ev in [
        Event::Start,
        Event::Resolved,
        Event::Transferred,
        Event::Verified,
        Event::Finalized,
    ] {
        transition(&c, id, ev).unwrap();
    }
    crate::history::record(&c, id, crate::history::Outcome::Completed).unwrap();
    remove(&mut c, id).unwrap();
    assert_eq!(crate::history::count(&c).unwrap(), 1);
}
