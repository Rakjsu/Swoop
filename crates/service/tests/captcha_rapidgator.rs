//! Rapidgator grátis pelo serviço inteiro, contra o Rapidgator falso: um
//! download por vez (o captcha pendente segura a vaga do servidor), aviso
//! com o prazo, contador nunca pulado, link pedido uma vez só, espera entre
//! downloads mostrada também na fila ("Aguardando") e captcha vencido que só
//! recomeça quando o usuário pede.

use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use swoop_api::{Backend, Command, Notice, Push, Subscription};
use swoop_core::{CaptchaKind, DownloadId, DownloadState, Settings};
use swoop_service::{Service, ServiceOptions};
use swoop_testsrv::{TestServer, effective_seed, fill};

const SIZE: u64 = 192 * 1024;
const ID: &str = "0123456789abcdef0123456789abcdef";

/// Serviço numa pasta temporária, tratando o Rapidgator falso como o site.
async fn open(dir: &Path, srv: &TestServer, extra: &str) -> Service {
    let data = dir.join("dados");
    std::fs::create_dir_all(data.join("rules")).unwrap();
    let rules = format!(
        "[rapidgator]\nhosts = [\"127.0.0.1\"]\norigin = \"{}\"\n{extra}",
        srv.rapidgator_origin()
    );
    std::fs::write(data.join("rules/hosts.toml"), rules).unwrap();
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

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

async fn add(svc: &Service, srv: &TestServer, name: &str, query: &str) -> DownloadId {
    let link = srv.rapidgator_url(ID, name, query);
    svc.add_links(&[link], None, "rg").await.unwrap()[0]
}

async fn state(svc: &Service, id: DownloadId) -> DownloadState {
    svc.row(id).await.unwrap().expect("download sumiu").state
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

/// Próximo aviso de captcha: segundos de prazo.
async fn captcha_deadline(sub: &mut Subscription) -> Option<u64> {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Some(Push::Notice(Notice::CaptchaNeeded { deadline_secs, .. })) =
                sub.next().await
            {
                return deadline_secs;
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
async fn um_por_vez_com_captcha_e_link_unico() {
    let srv = TestServer::start().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let svc = open(dir.path(), &srv, "").await;
    let mut sub = svc.subscribe();
    let q = |seed: u64| format!("secs=1&size={SIZE}&seed={seed}");
    let ids = [
        add(&svc, &srv, "um.bin", &q(11)).await,
        add(&svc, &srv, "dois.bin", &q(12)).await,
        add(&svc, &srv, "tres.bin", &q(13)).await,
    ];
    for (n, id) in ids.iter().enumerate() {
        let deadline = captcha_deadline(&mut sub).await.expect("aviso sem prazo");
        assert!((80..=85).contains(&deadline), "{deadline}");
        wait_state(&svc, *id, DownloadState::CaptchaNeeded, 10).await;
        // o captcha pendente segura a vaga do servidor: os outros esperam
        for other in &ids[n + 1..] {
            assert_eq!(state(&svc, *other).await, DownloadState::Queued);
        }
        let view = svc.list().await.unwrap();
        let info = view
            .iter()
            .find(|v| v.id == id.0)
            .unwrap()
            .captcha
            .clone()
            .unwrap();
        assert!(info.expires_at_ms.is_some_and(|t| t > now_ms()));
        let ch = svc.captcha_challenge(*id).await.unwrap().unwrap();
        assert!(matches!(ch.kind, CaptchaKind::Turnstile { .. }));
        svc.captcha_solved(*id, "ok".into()).await.unwrap();
        wait_state(&svc, *id, DownloadState::Completed, 30).await;
        assert_content(&svc, &srv, *id, 11 + n as u64).await;
    }
    let s = srv.rapidgator_stats();
    assert_eq!((s.early_link_calls, s.parallel_starts), (0, 0));
    assert_eq!(
        (s.links, s.final_link_gets, s.final_link_rejected),
        (3, 3, 0),
        "link pedido mais de uma vez"
    );
    svc.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn espera_entre_downloads_aparece_na_fila_e_sobrevive_a_reabrir() {
    let srv = TestServer::start().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let svc = open(dir.path(), &srv, "").await;
    let q = format!("secs=0&captcha=none&delay=120&size={SIZE}");
    let first = add(&svc, &srv, "um.bin", &q).await;
    wait_state(&svc, first, DownloadState::Completed, 30).await;
    let second = add(&svc, &srv, "dois.bin", &q).await;
    wait_state(&svc, second, DownloadState::Waiting, 10).await;
    let third = add(&svc, &srv, "tres.bin", &q).await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(state(&svc, third).await, DownloadState::Queued);
    let view = svc.list().await.unwrap();
    let until = |id: DownloadId| view.iter().find(|v| v.id == id.0).unwrap().wait_until_ms;
    let expected = now_ms() + 120 * 60 * 1000;
    for id in [second, third] {
        let t = until(id).expect("sem espera na lista");
        assert!(
            (t - expected).abs() < 10_000,
            "espera fora de 120 min: {t} vs {expected}"
        );
    }
    svc.shutdown().await;
    drop(svc);

    let svc = open(dir.path(), &srv, "").await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(
        state(&svc, third).await,
        DownloadState::Queued,
        "a espera não sobreviveu"
    );
    assert_eq!(srv.rapidgator_stats().links, 1);
    svc.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn captcha_vencido_espera_o_usuario_recomecar() {
    let srv = TestServer::start().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let svc = open(
        dir.path(),
        &srv,
        "captcha_deadline_secs = 2\ncaptcha_min_left_secs = 0\n",
    )
    .await;
    let q = format!("secs=0&size={SIZE}");
    let first = add(&svc, &srv, "um.bin", &q).await;
    wait_state(&svc, first, DownloadState::CaptchaNeeded, 10).await;
    let second = add(&svc, &srv, "dois.bin", &q).await;
    tokio::time::sleep(Duration::from_millis(2_300)).await;

    // a resposta depois do prazo é recusada e nada é enviado ao site
    let err = svc.captcha_solved(first, "ok".into()).await.unwrap_err();
    assert!(err.to_string().contains("Recomeçar"), "{err}");
    assert_eq!(state(&svc, first).await, DownloadState::CaptchaNeeded);
    // vencido não segura a vaga: o próximo anda; o vencido não recomeça sozinho
    wait_state(&svc, second, DownloadState::CaptchaNeeded, 10).await;
    assert_eq!(state(&svc, first).await, DownloadState::CaptchaNeeded);
    assert_eq!(srv.rapidgator_stats().timers, 2);

    // Recomeçar (retry) devolve à fila
    svc.exec(Command::Retry { id: first.0 }).await.unwrap();
    assert_eq!(state(&svc, first).await, DownloadState::Queued);
    let s = srv.rapidgator_stats();
    assert_eq!((s.submits, s.late_submits), (0, 0));
    svc.shutdown().await;
}
