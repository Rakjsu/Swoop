//! Fase 4a no motor: captcha pendente não segura vaga e a resposta chega ao
//! plugin; espera de "limite do servidor" vale para o servidor inteiro e
//! sobrevive a reabrir o app.

mod common;

use async_trait::async_trait;
use common::*;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};
use swoop_core::{
    CaptchaChallenge, CaptchaKind, DownloadState, Event, HostError, ResolveRequest, Resolved,
    Resolver, WaitReason,
};
use swoop_store::{captcha, downloads};
use swoop_testsrv::TestServer;
use url::Url;

/// Plugin de teste: `captcha.test` pede captcha até receber o token `ok`;
/// `limite.test` manda esperar 2 s na 1ª vez; o resto baixa do testsrv.
struct Fake {
    file: String,
    tokens: Mutex<Vec<String>>,
    limited: Mutex<bool>,
}

fn challenge(url: &Url) -> CaptchaChallenge {
    CaptchaChallenge {
        kind: CaptchaKind::Recaptcha2 {
            site_key: "chave".into(),
        },
        page_url: url.clone(),
        action: url.clone(),
        fields: vec![("op".into(), "download2".into())],
        answer_field: "g-recaptcha-response".into(),
        not_before_ms: 0,
    }
}

#[async_trait]
impl Resolver for Fake {
    async fn resolve(&self, req: &ResolveRequest) -> Result<Resolved, HostError> {
        // Um arquivo por link (nome = último trecho do caminho).
        let name = req.url.path().trim_start_matches('/').replace('/', "-");
        let direct = Resolved::direct(Url::parse(&self.file.replace("NOME", &name)).unwrap());
        match req.url.host_str() {
            Some("captcha.test") => match &req.captcha {
                Some(answer) => {
                    self.tokens.lock().unwrap().push(answer.token.clone());
                    assert_eq!(answer.challenge.page_url, req.url);
                    Ok(direct)
                }
                None => Err(HostError::Captcha(Box::new(challenge(&req.url)))),
            },
            Some("limite.test") if !std::mem::replace(&mut *self.limited.lock().unwrap(), true) => {
                Err(HostError::Wait {
                    until: SystemTime::now() + Duration::from_secs(2),
                    reason: WaitReason::HostLimit,
                })
            }
            _ => Ok(direct),
        }
    }
}

async fn wait_state(h: &Harness, id: swoop_core::DownloadId, want: DownloadState) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while h.row(id).await.state != want {
        assert!(
            Instant::now() < deadline,
            "não chegou em {want}: {:?}",
            h.row(id).await.state
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn captcha_espera_o_usuario_sem_segurar_vaga() {
    let srv = TestServer::start().await.unwrap();
    let fake = std::sync::Arc::new(Fake {
        file: srv.file_url("NOME.bin", "size=65536"),
        tokens: Mutex::new(Vec::new()),
        limited: Mutex::new(false),
    });
    let mut s = settings(2);
    s.max_active_downloads = 1;
    let h = Harness::with_resolver(s, fake.clone()).await;

    let cap = h.add("https://captcha.test/abc".into()).await;
    wait_state(&h, cap, DownloadState::CaptchaNeeded).await;
    assert!(h.row(cap).await.captcha.is_some(), "desafio gravado");

    // Com uma vaga só, outro download anda enquanto o captcha espera.
    let other = h.add("https://outro.test/b".into()).await;
    completed(&h.wait_final(&[other], Duration::from_secs(20)).await[0]);

    // O usuário resolve: token guardado, volta para a fila e o plugin recebe.
    h.store
        .call(move |c| {
            captcha::set_token(c, cap, "ok")?;
            downloads::transition(c, cap, Event::CaptchaSolved)
        })
        .await
        .unwrap();
    h.engine.wake();
    completed(&h.wait_final(&[cap], Duration::from_secs(20)).await[0]);
    assert_eq!(*fake.tokens.lock().unwrap(), vec!["ok".to_owned()]);
    assert!(
        h.row(cap).await.captcha.is_none(),
        "resposta usada uma vez só"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn espera_do_servidor_vale_para_todos_e_sobrevive_a_reabrir() {
    let srv = TestServer::start().await.unwrap();
    let fake = std::sync::Arc::new(Fake {
        file: srv.file_url("NOME.bin", "size=65536"),
        tokens: Mutex::new(Vec::new()),
        limited: Mutex::new(false),
    });
    let mut h = Harness::with_resolver(settings(2), fake).await;

    let t0 = Instant::now();
    let first = h.add("https://limite.test/1".into()).await;
    wait_state(&h, first, DownloadState::Waiting).await;
    let second = h.add("https://limite.test/2".into()).await;
    // Fecha e reabre no meio da espera: o servidor continua bloqueado.
    h.reopen(settings(2)).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        h.row(second).await.state,
        DownloadState::Queued,
        "não pode começar antes"
    );

    let rows = h
        .wait_final(&[first, second], Duration::from_secs(20))
        .await;
    completed(&rows[0]);
    completed(&rows[1]);
    let started = rows[1].started_at.unwrap();
    let wait_end = rows[0].started_at.unwrap() + 2_000;
    assert!(
        started >= wait_end - 100,
        "o 2º começou antes do fim da espera do servidor"
    );
    assert!(t0.elapsed() >= Duration::from_secs(2));
}
