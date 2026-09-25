//! Painel em loopback com o serviço de verdade: segurança da API, arquivos
//! da UI e o portão (b) da fase 2 (45–55 retratos em 10 s pelo WebSocket,
//! sem evento por pedaço baixado).

use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::{Duration, Instant};
use swoop_api::Settings;
use swoop_net::reqwest::{Client, StatusCode, header};
use swoop_remote::{RemoteOptions, Running};
use swoop_service::{Service, ServiceOptions};
use swoop_testsrv::TestServer;
use tokio_tungstenite::tungstenite::Message;

const MIB: u64 = 1024 * 1024;

struct Panel {
    service: Arc<Service>,
    running: Running,
    http: Client,
    _dir: tempfile::TempDir,
}

impl Panel {
    /// Serviço numa pasta temporária + painel com uma UI mínima.
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let ui = dir.path().join("ui");
        std::fs::create_dir_all(&ui).unwrap();
        std::fs::write(
            ui.join("index.html"),
            "<!doctype html><title>painel</title>",
        )
        .unwrap();
        let service = Arc::new(
            Service::open(ServiceOptions {
                data_dir: Some(dir.path().join("dados")),
                settings: Some(Settings {
                    download_dir: Some(dir.path().join("baixados")),
                    ..Settings::default()
                }),
            })
            .await
            .unwrap(),
        );
        let running = swoop_remote::start(
            service.clone(),
            RemoteOptions {
                port: 0,
                ui_dir: Some(ui),
            },
        )
        .await
        .unwrap();
        let http = swoop_net::download_client("swoop-teste").unwrap();
        Self {
            service,
            running,
            http,
            _dir: dir,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.running.addr)
    }

    fn bearer(&self) -> String {
        format!("Bearer {}", self.running.token.as_str())
    }

    async fn stop(self) {
        self.running.stop().await;
        self.service.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn api_exige_token() {
    let p = Panel::start().await;
    let list = p.url("/api/v1/list");
    let res = p.http.get(&list).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let res = p
        .http
        .get(&list)
        .header(header::AUTHORIZATION, "Bearer 00ff")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let res = p
        .http
        .get(&list)
        .header(header::AUTHORIZATION, p.bearer())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.text().await.unwrap(), "[]");
    assert!(p.running.url().contains("/#t="));
    p.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn host_e_origin_de_fora_sao_recusados() {
    let p = Panel::start().await;
    let evil_host = p
        .http
        .get(p.url("/api/v1/list"))
        .header(header::AUTHORIZATION, p.bearer())
        .header(header::HOST, "evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(evil_host.status(), StatusCode::FORBIDDEN);
    let evil_origin = p
        .http
        .post(p.url("/api/v1/exec"))
        .header(header::AUTHORIZATION, p.bearer())
        .header(header::ORIGIN, "http://evil.example")
        .header(header::CONTENT_TYPE, "application/json")
        .body(r#"{"type":"pause_all"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(evil_origin.status(), StatusCode::FORBIDDEN);
    let page = p
        .http
        .get(p.url("/"))
        .header(header::HOST, "evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::FORBIDDEN);
    p.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn exec_e_arquivos_da_ui() {
    let p = Panel::start().await;
    let bad = p
        .http
        .post(p.url("/api/v1/exec"))
        .header(header::AUTHORIZATION, p.bearer())
        .header(header::CONTENT_TYPE, "application/json")
        .body(r#"{"type":"add_links","links":["não é link"]}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = serde_json::from_slice(&bad.bytes().await.unwrap()).unwrap();
    assert_eq!(body["kind"], "invalid");

    let page = p.http.get(p.url("/")).send().await.unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    assert!(page.headers().contains_key(header::CONTENT_SECURITY_POLICY));
    assert!(page.text().await.unwrap().contains("painel"));
    // rota da UI sem arquivo cai no index; nada fora da pasta da UI é servido
    let spa = p.http.get(p.url("/historico")).send().await.unwrap();
    assert!(spa.text().await.unwrap().contains("painel"));
    let escape = p
        .http
        .get(p.url("/%2e%2e/dados/swoop.lock"))
        .send()
        .await
        .unwrap();
    assert!(escape.text().await.unwrap().contains("painel"));
    p.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn websocket_portao_b() {
    let srv = TestServer::start().await.unwrap();
    let p = Panel::start().await;
    let ws_url = format!("ws://{}/api/v1/ws", p.running.addr);

    // token errado: o servidor fecha
    let (mut bad, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();
    bad.send(Message::text("errado")).await.unwrap();
    let closed = tokio::time::timeout(Duration::from_secs(5), bad.next()).await;
    assert!(matches!(closed, Ok(Some(Ok(Message::Close(_))) | None)));

    let (mut ws, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();
    ws.send(Message::text(p.running.token.as_str()))
        .await
        .unwrap();
    // 64 MiB a 256 KiB/s por conexão (8 conexões ≈ 2 MiB/s): dura a janela toda
    let url = srv.file_url(
        "longo.bin",
        &format!("size={}&seed=5&rate={}", 64 * MIB, 256 * 1024),
    );
    let res = p
        .http
        .post(p.url("/api/v1/exec"))
        .header(header::AUTHORIZATION, p.bearer())
        .header(header::CONTENT_TYPE, "application/json")
        .body(serde_json::json!({ "type": "add_links", "links": [url] }).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // espera o download andar e conta 10 s de mensagens
    tokio::time::sleep(Duration::from_secs(2)).await;
    while let Ok(Some(_)) = tokio::time::timeout(Duration::from_millis(1), ws.next()).await {}
    let (mut snapshots, mut others) = (0, 0);
    let end = Instant::now() + Duration::from_secs(10);
    while let Ok(Some(Ok(msg))) = tokio::time::timeout_at(end.into(), ws.next()).await {
        let Message::Text(text) = msg else { continue };
        let push: serde_json::Value = serde_json::from_str(text.as_str()).unwrap();
        if push["type"] == "snapshot" {
            snapshots += 1;
            assert!(push["speed_bps"].as_u64().is_some());
        } else {
            others += 1;
            eprintln!("aviso fora do retrato: {text}");
        }
    }
    println!("portão 2b: {snapshots} retratos e {others} outros avisos em 10 s");
    assert!(
        (45..=55).contains(&snapshots),
        "{snapshots} retratos em 10 s"
    );
    // download estável: nenhuma mudança de estado, logo nenhum outro aviso
    assert!(
        others <= 1,
        "{others} avisos fora os retratos: evento por pedaço?"
    );
    p.stop().await;
}
