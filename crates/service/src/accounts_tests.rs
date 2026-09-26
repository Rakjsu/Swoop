//! Contas premium pelo serviço, contra o XFS falso do testsrv (cofre em
//! memória). Portões da 4b: (a) premium baixa com 5 conexões e retoma depois
//! de fechar e reabrir, arquivo idêntico; (b) o segredo não aparece nos
//! arquivos da pasta de dados (banco, WAL) nem nos logs.

use super::*;
use crate::ServiceOptions;
use crate::vault::memory;
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};
use swoop_core::{DownloadState, Settings};
use swoop_testsrv::TestServer;
use swoop_testsrv::xfs_premium::{KEY, PASS, USER};

const MIB: u64 = 1024 * 1024;

/// Tudo o que o `tracing` escreveu neste processo de teste.
static LOGS: Mutex<Vec<u8>> = Mutex::new(Vec::new());

struct Capture;

impl Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        LOGS.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Liga a captura dos logs (nível debug, todos os crates) uma vez só.
fn capture_logs() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("debug")
            .with_writer(|| Capture)
            .try_init();
    });
}

/// Serviço numa pasta temporária, tratando o testsrv (http) como XFS.
async fn open(dir: &Path) -> Service {
    let data = dir.join("dados");
    std::fs::create_dir_all(data.join("rules")).unwrap();
    std::fs::write(
        data.join("rules/hosts.toml"),
        "[xfs]\nhosts = [\"127.0.0.1\"]\naccount_scheme = \"http\"\n",
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

fn host(srv: &TestServer) -> String {
    format!("127.0.0.1:{}", srv.addr().port())
}

async fn row(svc: &Service, host: &str) -> AccountRow {
    let rows = svc.account_rows().await.unwrap();
    rows.into_iter()
        .find(|r| r.host_key == host)
        .expect("conta")
}

/// Todos os bytes dos arquivos de `dir` (recursivo) contêm `needle`?
fn found_in_files(dir: &Path, needle: &[u8]) -> Vec<String> {
    let mut hits = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            hits.extend(found_in_files(&path, needle));
        } else if let Ok(bytes) = std::fs::read(&path)
            && bytes.windows(needle.len()).any(|w| w == needle)
        {
            hits.push(path.display().to_string());
        }
    }
    hits
}

#[tokio::test(flavor = "multi_thread")]
async fn cadastro_confere_troca_e_remove_limpando_o_cofre() {
    let srv = TestServer::start().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let svc = open(dir.path()).await;
    let host = host(&srv);

    let err = svc
        .add_account("outro.example", "", AccountKind::ApiKey, Secret::new("k"))
        .await;
    assert!(matches!(err, Err(ServiceError::BadInput(_))));
    let err = svc
        .add_account(&host, "", AccountKind::Login, Secret::new("x"))
        .await;
    assert!(matches!(err, Err(ServiceError::BadInput(m)) if m.contains("usuário")));

    svc.add_account(&host, "", AccountKind::ApiKey, Secret::new(KEY))
        .await
        .unwrap();
    assert_eq!(
        svc.check_account(&host).await.unwrap(),
        AccountStatus::Premium
    );
    let first = row(&svc, &host).await;
    assert!(first.premium_until.is_some() && first.checked_at.is_some());
    assert!(memory::shared().contains(&first.secret_ref));

    // Trocar por login: o segredo antigo sai do cofre.
    svc.add_account(&host, USER, AccountKind::Login, Secret::new(PASS))
        .await
        .unwrap();
    let second = row(&svc, &host).await;
    assert!(!memory::shared().contains(&first.secret_ref));
    assert_eq!(
        svc.check_account(&host).await.unwrap(),
        AccountStatus::Premium
    );

    // Senha errada fica marcada, com o motivo.
    svc.add_account(&host, USER, AccountKind::Login, Secret::new("errada"))
        .await
        .unwrap();
    assert_eq!(
        svc.check_account(&host).await.unwrap(),
        AccountStatus::Invalid
    );
    let bad = row(&svc, &host).await;
    assert!(bad.error.unwrap().contains("usuário ou a senha"));
    assert!(!memory::shared().contains(&second.secret_ref));

    svc.remove_account(&host).await.unwrap();
    assert!(svc.account_rows().await.unwrap().is_empty());
    assert!(!memory::shared().contains(&bad.secret_ref));
    assert!(svc.hosts.accounts().get(&host).is_none());
    svc.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn premium_baixa_com_5_conexoes_retoma_e_nao_vaza_o_segredo() {
    capture_logs();
    let srv = TestServer::start().await.unwrap();
    // 24 MiB a 1 MiB/s por conexão: dá tempo de fechar no meio.
    srv.set_xfs_premium_file(&format!("size={}&seed=7&rate={MIB}", 24 * MIB));
    let dir = tempfile::tempdir().unwrap();
    let host = host(&srv);
    let secret = "chave-premium";
    assert_eq!(secret, KEY);

    let svc = open(dir.path()).await;
    svc.add_account(&host, "", AccountKind::ApiKey, Secret::new(KEY))
        .await
        .unwrap();
    // contador de 30 s e captcha no grátis: o premium não passa por eles
    let link = srv.xfs_url("abcdefgh1234", "countdown=30");
    let id = svc.add_links(&[link], None, "premium").await.unwrap()[0];

    // Fecha no meio (≥ 25%) e reabre: a conta volta do cofre e o download
    // continua do ponto.
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

    let svc = open(dir.path()).await;
    assert!(
        svc.hosts.accounts().get(&host).is_some(),
        "conta voltou do cofre"
    );
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
        "premium: pico de {} conexões; fechado com {:.1} MiB; {:.1} MiB enviados para 24 MiB",
        stats.peak_connections,
        before as f64 / MIB as f64,
        stats.bytes_sent as f64 / MIB as f64
    );
    assert_eq!(stats.peak_connections, 5, "premium = 5 conexões");
    assert!(
        stats.bytes_sent < 24 * MIB + before / 2 + 4 * MIB,
        "retomou do ponto (enviados {} MiB)",
        stats.bytes_sent / MIB
    );
    let got = std::fs::read(done.final_path.unwrap()).unwrap();
    let mut want = vec![0u8; (24 * MIB) as usize];
    swoop_testsrv::fill(
        swoop_testsrv::effective_seed(7, srv.generation()),
        0,
        &mut want,
    );
    assert!(got == want, "arquivo diferente do servidor");
    assert_eq!(srv.xfs_stats().links, 0, "nada pelo fluxo grátis");
    svc.shutdown().await;
    drop(svc);

    // Portão (b): o segredo não está em nenhum arquivo da pasta de dados
    // (banco, WAL) nem nos logs.
    let hits = found_in_files(&dir.path().join("dados"), secret.as_bytes());
    assert!(hits.is_empty(), "segredo em disco: {hits:?}");
    let logs = LOGS.lock().unwrap();
    assert!(!logs.is_empty(), "a captura de logs não pegou nada");
    assert!(
        !logs.windows(secret.len()).any(|w| w == secret.as_bytes()),
        "segredo no log"
    );
}
