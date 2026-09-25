//! O servidor de teste precisa estar certo antes de servir de régua ao motor.

use swoop_testsrv::{TestServer, effective_seed, fill};

#[tokio::test]
async fn range_if_range_e_contadores() {
    let srv = TestServer::start().await.unwrap();
    let client = swoop_net::download_client("teste").unwrap();
    let url = srv.file_url("a.bin", "size=1000&seed=9");

    let resp = client
        .get(&url)
        .header("Range", "bytes=10-19")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 206);
    assert_eq!(resp.headers()["content-range"], "bytes 10-19/1000");
    let etag = resp.headers()["etag"].to_str().unwrap().to_owned();
    let body = resp.bytes().await.unwrap();
    let mut expected = vec![0u8; 10];
    fill(effective_seed(9, 0), 10, &mut expected);
    assert_eq!(&body[..], &expected[..]);

    // If-Range com ETag antigo depois de trocar a geração: arquivo inteiro (200).
    srv.set_generation(1);
    let resp = client
        .get(&url)
        .header("Range", "bytes=10-19")
        .header("If-Range", etag)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.bytes().await.unwrap().len(), 1000);

    let resp = client
        .get(&url)
        .header("Range", "bytes=5000-")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 416);

    let norange = srv.file_url("b.bin", "size=100&norange=1");
    let resp = client
        .get(&norange)
        .header("Range", "bytes=0-0")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert!(resp.headers().get("accept-ranges").is_none());
    let _ = resp.bytes().await.unwrap();

    assert_eq!(srv.stats().active_connections, 0);
    assert!(srv.stats().peak_connections >= 1);
}

#[tokio::test]
async fn taxa_por_conexao_e_queda() {
    let srv = TestServer::start().await.unwrap();
    let client = swoop_net::download_client("teste").unwrap();

    let t0 = std::time::Instant::now();
    let url = srv.file_url("lento.bin", "size=65536&rate=131072");
    let body = client
        .get(&url)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert_eq!(body.len(), 65536);
    let secs = t0.elapsed().as_secs_f64();
    assert!(
        (0.4..1.2).contains(&secs),
        "64 KiB a 128 KiB/s levou {secs:.2}s"
    );

    let url = srv.file_url("cai.bin", "size=100000&drop_after=5000");
    // A queda pode chegar antes dos cabeçalhos (hyper ainda não enviou) ou no corpo.
    let result = async { client.get(&url).send().await?.bytes().await }.await;
    assert!(result.is_err(), "a conexão deveria cair no meio");
}
