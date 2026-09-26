//! Portão 4a (b): download grátis de XFileSharing pelo serviço inteiro, contra
//! o XFS falso do testsrv. Captcha pendente vira aviso e aparece na lista;
//! resposta errada pede de novo; o contador do site nunca é pulado; a espera
//! entre downloads segura o servidor; os arquivos chegam inteiros.

use std::path::Path;
use std::time::{Duration, Instant};
use swoop_api::{Backend, Notice, Push, Subscription};
use swoop_core::{CaptchaKind, DownloadId, DownloadState, Settings};
use swoop_service::{Service, ServiceOptions};
use swoop_testsrv::{TestServer, effective_seed, fill};

const SIZE: u64 = 256 * 1024;

/// Serviço numa pasta temporária, tratando o testsrv como site XFileSharing.
async fn open(dir: &Path) -> Service {
    let data = dir.join("dados");
    std::fs::create_dir_all(data.join("rules")).unwrap();
    std::fs::write(
        data.join("rules/hosts.toml"),
        "[xfs]\nhosts = [\"127.0.0.1\"]\n",
    )
    .unwrap();
    Service::open(ServiceOptions {
        data_dir: Some(data),
        settings: Some(Settings {
            download_dir: Some(dir.join("baixados")),
            ..Settings::default()
        }),
    })
    .await
    .unwrap()
}

/// Espera o download chegar em `want`; falha se ele terminar em outro estado.
async fn wait_state(svc: &Service, id: DownloadId, want: DownloadState, secs: u64) {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        let row = svc.row(id).await.unwrap().expect("download sumiu");
        if row.state == want {
            return;
        }
        assert!(
            !row.state.is_final(),
            "terminou em {} ({:?})",
            row.state,
            row.error_msg
        );
        assert!(
            Instant::now() < deadline,
            "não chegou em {want}: {}",
            row.state
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Próximo aviso de captcha: (nome, site).
async fn captcha_notice(sub: &mut Subscription) -> (String, String) {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Some(Push::Notice(Notice::CaptchaNeeded { name, host })) = sub.next().await {
                return (name, host);
            }
        }
    })
    .await
    .expect("nenhum aviso de captcha")
}

/// O arquivo baixado é igual, byte a byte, ao que o testsrv serve.
async fn assert_content(svc: &Service, srv: &TestServer, id: DownloadId, seed: u64) {
    let row = svc.row(id).await.unwrap().unwrap();
    let got = std::fs::read(row.final_path.expect("sem caminho final")).unwrap();
    let mut want = vec![0u8; SIZE as usize];
    fill(effective_seed(seed, srv.generation()), 0, &mut want);
    assert!(got == want, "conteúdo diferente do servidor");
}

#[tokio::test(flavor = "multi_thread")]
async fn captcha_contador_e_espera_do_xfs_pelo_servico() {
    let srv = TestServer::start().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let svc = open(dir.path()).await;
    let mut sub = svc.subscribe();

    // 1º: reCAPTCHA com contador de 2 s; depois o site pede 5 s de intervalo.
    let q = format!("countdown=2&wait_between=5&size={SIZE}&seed=5");
    let first = svc
        .add_links(&[srv.xfs_url("aaaaaaaa0001", &q)], None, "xfs")
        .await
        .unwrap()[0];
    let (_, host) = captcha_notice(&mut sub).await;
    assert_eq!(host, "127.0.0.1");
    wait_state(&svc, first, DownloadState::CaptchaNeeded, 10).await;
    let view = svc.list().await.unwrap();
    let info = view
        .iter()
        .find(|v| v.id == first.0)
        .unwrap()
        .captcha
        .clone();
    assert_eq!(info.expect("captcha na lista").host, "127.0.0.1");
    let ch = svc.captcha_challenge(first).await.unwrap().unwrap();
    assert!(
        matches!(ch.kind, CaptchaKind::Recaptcha2 { ref site_key } if site_key == "test-site-key")
    );

    // Resposta errada logo de cara: o site recusa e o captcha volta.
    svc.captcha_solved(first, "errado".into()).await.unwrap();
    captcha_notice(&mut sub).await;
    wait_state(&svc, first, DownloadState::CaptchaNeeded, 10).await;
    assert_eq!(srv.xfs_stats().wrong_answers, 1);

    // Resposta certa na hora: o envio espera o contador e o download sai.
    let t = Instant::now();
    svc.captcha_solved(first, "ok".into()).await.unwrap();
    wait_state(&svc, first, DownloadState::Completed, 30).await;
    assert!(
        t.elapsed() >= Duration::from_millis(1500),
        "enviou antes do contador"
    );
    assert_content(&svc, &srv, first, 5).await;

    // 2º (captcha de imagem): o intervalo do site segura o servidor antes
    // de pedir o captcha.
    let t2 = Instant::now();
    let q2 = format!("countdown=1&captcha=image&size={SIZE}&seed=6");
    let second = svc
        .add_links(&[srv.xfs_url("aaaaaaaa0002", &q2)], None, "xfs")
        .await
        .unwrap()[0];
    wait_state(&svc, second, DownloadState::Waiting, 10).await;
    wait_state(&svc, second, DownloadState::CaptchaNeeded, 30).await;
    assert!(
        t2.elapsed() >= Duration::from_secs(4),
        "pediu o captcha durante o intervalo do site"
    );
    let ch = svc.captcha_challenge(second).await.unwrap().unwrap();
    assert!(matches!(ch.kind, CaptchaKind::Image { .. }));
    assert_eq!(ch.answer_field, "code");
    svc.captcha_solved(second, "ok".into()).await.unwrap();
    wait_state(&svc, second, DownloadState::Completed, 30).await;
    assert_content(&svc, &srv, second, 6).await;

    let stats = srv.xfs_stats();
    assert_eq!(stats.early_submits, 0, "contador pulado");
    assert_eq!(stats.wrong_answers, 1);
    assert_eq!(stats.links, 2);
    // Resposta para quem não espera captcha, ou vazia, é recusada.
    assert!(svc.captcha_solved(second, "ok".into()).await.is_err());
    assert!(svc.captcha_challenge(second).await.unwrap().is_none());
    svc.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn resposta_vazia_e_recusada_sem_mexer_no_download() {
    let srv = TestServer::start().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let svc = open(dir.path()).await;
    let q = format!("countdown=0&size={SIZE}");
    let id = svc
        .add_links(&[srv.xfs_url("aaaaaaaa0003", &q)], None, "xfs")
        .await
        .unwrap()[0];
    wait_state(&svc, id, DownloadState::CaptchaNeeded, 10).await;
    assert!(svc.captcha_solved(id, "  ".into()).await.is_err());
    assert!(svc.captcha_solved(id, "a\nb".into()).await.is_err());
    let row = svc.row(id).await.unwrap().unwrap();
    assert_eq!(row.state, DownloadState::CaptchaNeeded);
    assert!(row.captcha.is_some(), "desafio continua guardado");
    svc.shutdown().await;
}
