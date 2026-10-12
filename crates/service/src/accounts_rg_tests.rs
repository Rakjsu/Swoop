//! Conta premium do Rapidgator pelo serviço, contra a API v2 falsa (cofre em
//! memória): só "usuário e senha" é aceito; premium baixa com 5 conexões,
//! retoma depois de fechar e reabrir, confere o MD5 da API; senha e token
//! não aparecem em arquivo nenhum da pasta de dados nem nos logs.

use super::tests::{LOGS, capture_logs, found_in_files};
use super::*;
use crate::ServiceOptions;
use std::path::Path;
use std::time::{Duration, Instant};
use swoop_core::{DownloadState, Settings};
use swoop_testsrv::TestServer;
use swoop_testsrv::rapidgator_api::{PASSWORD, PREMIUM_USER};

const MIB: u64 = 1024 * 1024;
const ID: &str = "0123456789abcdef0123456789abcdef";

async fn open(dir: &Path, srv: &TestServer) -> Service {
    let data = dir.join("dados");
    std::fs::create_dir_all(data.join("rules")).unwrap();
    let rules = format!(
        "[rapidgator]\nhosts = [\"127.0.0.1\"]\norigin = \"{}\"\n",
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

#[tokio::test(flavor = "multi_thread")]
async fn rapidgator_premium_5_conexoes_retoma_md5_e_sem_segredo_em_disco() {
    capture_logs();
    let srv = TestServer::start().await.unwrap();
    srv.rapidgator().set_premium_file(24 * MIB, 17, Some(MIB));
    let dir = tempfile::tempdir().unwrap();
    let host = srv
        .rapidgator_origin()
        .trim_start_matches("http://")
        .to_owned();

    let svc = open(dir.path(), &srv).await;
    let hosts = svc.account_hosts();
    let rg = hosts
        .iter()
        .find(|h| h.host == host)
        .expect("Rapidgator na aba Contas");
    assert_eq!(rg.kinds, vec![AccountKind::Login]);
    let err = svc
        .add_account(&host, "", AccountKind::ApiKey, Secret::new("chave"))
        .await;
    assert!(matches!(err, Err(ServiceError::BadInput(m)) if m.contains("senha")));
    svc.add_account(
        &host,
        PREMIUM_USER,
        AccountKind::Login,
        Secret::new(PASSWORD),
    )
    .await
    .unwrap();
    assert_eq!(
        svc.check_account(&host).await.unwrap(),
        AccountStatus::Premium
    );

    // contador de 30 s e captcha no grátis: o premium não passa por eles
    let link = srv.rapidgator_url(ID, "premium.bin", "secs=30");
    let id = svc.add_links(&[link], None, "premium").await.unwrap()[0];
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let r = svc.row(id).await.unwrap().unwrap();
        if r.done_bytes >= 6 * MIB {
            break;
        }
        assert!(
            !r.state.is_final(),
            "terminou cedo: {} {:?}",
            r.state,
            r.error_msg
        );
        assert!(Instant::now() < deadline, "não andou");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    svc.shutdown().await;
    let before = svc.row(id).await.unwrap().unwrap().done_bytes;
    drop(svc);

    let svc = open(dir.path(), &srv).await;
    let deadline = Instant::now() + Duration::from_secs(60);
    let done = loop {
        let r = svc.row(id).await.unwrap().unwrap();
        if r.state == DownloadState::Completed {
            break r;
        }
        assert!(!r.state.is_final(), "falhou: {:?}", r.error_msg);
        assert!(Instant::now() < deadline, "não terminou");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let stats = srv.stats();
    eprintln!(
        "rapidgator premium: pico de {} conexões; fechado com {:.1} MiB; {:.1} MiB enviados para 24 MiB",
        stats.peak_connections,
        before as f64 / MIB as f64,
        stats.bytes_sent as f64 / MIB as f64
    );
    assert_eq!(stats.peak_connections, 5, "premium = 5 conexões");
    assert!(
        stats.bytes_sent <= 24 * MIB + 4 * MIB,
        "retomou do ponto (enviados {} MiB)",
        stats.bytes_sent / MIB
    );
    let got = std::fs::read(done.final_path.unwrap()).unwrap();
    let mut want = vec![0u8; (24 * MIB) as usize];
    swoop_testsrv::fill(
        swoop_testsrv::effective_seed(17, srv.generation()),
        0,
        &mut want,
    );
    assert!(got == want, "arquivo diferente do servidor");
    let rg = srv.rapidgator_stats();
    assert_eq!((rg.timers, rg.password_in_query), (0, 0));
    svc.shutdown().await;
    drop(svc);

    // senha e token (tok-api-N) fora dos arquivos e dos logs
    let data = dir.path().join("dados");
    for secret in [PASSWORD, "tok-api-"] {
        let hits = found_in_files(&data, secret.as_bytes());
        assert!(hits.is_empty(), "{secret} em disco: {hits:?}");
        let logs = LOGS.lock().unwrap();
        assert!(
            !logs.windows(secret.len()).any(|w| w == secret.as_bytes()),
            "{secret} no log"
        );
    }
}
