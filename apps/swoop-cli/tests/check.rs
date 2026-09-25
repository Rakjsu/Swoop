//! `swoop-cli check`: acha os links no texto, confere cada um sem baixar e
//! sai com 1 quando algum não está online.

use std::process::Command;
use swoop_testsrv::TestServer;

#[tokio::test(flavor = "multi_thread")]
async fn confere_links_no_texto() {
    let srv = TestServer::start().await.unwrap();
    let online = srv.file_url("filme.mkv", "size=3145728&cd=1");
    let removed = online.replace("/file/", "/sumiu/");
    let data = tempfile::tempdir().unwrap();
    let text = format!("baixe {online} e também {removed}.");
    let out = tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_swoop-cli"))
            .args(["check", &text, "--data-dir"])
            .arg(data.path())
            .env("RUST_LOG", "warn")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "saída: {stdout}");
    assert!(lines[0].starts_with("direct") && lines[0].contains("online"));
    assert!(lines[0].contains("3.0 MiB") && lines[0].ends_with("filme.mkv"));
    assert!(lines[1].contains("offline"), "{}", lines[1]);
    assert_eq!(out.status.code(), Some(1));
    // Só a sonda de 1 byte: nada foi baixado.
    assert!(srv.stats().bytes_sent <= 1);
}
