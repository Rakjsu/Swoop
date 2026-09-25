//! Portão 3c, parte dos plugins: links públicos de verdade (rede; `#[ignore]`).
//!
//! `cargo test -p swoop-hosts --test network -- --ignored --test-threads=1 --nocapture`
//!
//! Cada link pode ser trocado por variável de ambiente (`SWOOP_TEST_*`) se
//! sair do ar. Origem dos links padrão:
//! - Drive: conjunto de testes do gdown (github.com/wkentaro/gdown, MIT);
//! - Mediafire: README do mediafiredl e testes do md_downloader (MIT);
//! - Pixeldrain: exemplo do Pixeldrain-Direct-Download-Terminal.

use std::time::{Duration, Instant};
use swoop_core::{HostError, ResolveRequest, Resolver};
use swoop_hosts::{Registry, Rules};
use url::Url;

/// Link da variável de ambiente ou o padrão.
fn link(var: &str, default: &str) -> Url {
    let s = std::env::var(var).unwrap_or_else(|_| default.to_owned());
    Url::parse(&s).expect("link de teste inválido")
}

fn registry() -> Registry {
    Registry::new(Rules::embedded()).unwrap()
}

async fn resolve(reg: &Registry, url: Url) -> Result<swoop_core::Resolved, HostError> {
    reg.resolve(&ResolveRequest { url, attempt: 0 }).await
}

/// Link removido/inexistente vira Offline em até 5 s.
async fn assert_offline_fast(reg: &Registry, url: &str) {
    let t0 = Instant::now();
    let res = reg.check(&Url::parse(url).unwrap()).await;
    assert_eq!(res, Err(HostError::Offline), "{url}");
    assert!(
        t0.elapsed() <= Duration::from_secs(5),
        "demorou {:?}",
        t0.elapsed()
    );
}

#[tokio::test]
#[ignore = "rede"]
async fn pixeldrain_publico_e_removido() {
    let reg = registry();
    let url = link("SWOOP_TEST_PIXELDRAIN", "https://pixeldrain.com/u/cW8FKre1");
    let info = reg.check(&url).await.expect("check do Pixeldrain");
    println!("pixeldrain: {info:?}");
    assert!(info.size.unwrap_or(0) > 0);
    let r = resolve(&reg, url).await.expect("resolve do Pixeldrain");
    assert_eq!(r.host_key, "pixeldrain");
    assert!(r.url.query().is_some_and(|q| q.contains("download")));
    assert_offline_fast(&reg, "https://pixeldrain.com/u/zzzzzzzzzz").await;
}

#[tokio::test]
#[ignore = "rede"]
async fn mediafire_publico_com_sha256_e_removido() {
    let reg = registry();
    let url = link(
        "SWOOP_TEST_MEDIAFIRE",
        "https://www.mediafire.com/file/ipnyzofjcwri357/test-10mb.bin/file",
    );
    let info = reg.check(&url).await.expect("check do Mediafire");
    println!("mediafire: {info:?}");
    assert!(info.size.unwrap_or(0) > 0);
    let r = resolve(&reg, url).await.expect("resolve do Mediafire");
    println!("mediafire: servidor final {:?}", r.url.host_str());
    assert!(
        r.url
            .host_str()
            .is_some_and(|h| h.ends_with("mediafire.com"))
    );
    assert!(r.integrity.is_some(), "a API não trouxe o sha256");
    assert_offline_fast(
        &reg,
        "https://www.mediafire.com/file/zzzzzzzzzzzzzzz/nada.zip/file",
    )
    .await;
}

#[tokio::test]
#[ignore = "rede"]
async fn mediafire_arquivo_perigoso_pede_o_navegador() {
    let reg = registry();
    let url = link(
        "SWOOP_TEST_MEDIAFIRE_DANGER",
        "https://www.mediafire.com/file/7f8x0azhs3pb1wm",
    );
    let res = resolve(&reg, url).await;
    assert!(matches!(res, Err(HostError::BrowserRequired(_))), "{res:?}");
}

#[tokio::test]
#[ignore = "rede"]
async fn mediafire_pasta_com_subpastas() {
    let reg = registry();
    let url = link(
        "SWOOP_TEST_MEDIAFIRE_FOLDER",
        "https://www.mediafire.com/folder/akjcex4b8dgui",
    );
    let files = reg
        .expand(&url)
        .await
        .unwrap()
        .expect("não abriu como pasta");
    println!("mediafire: pasta com {} arquivo(s)", files.len());
    // Contagem do teste do md_downloader (jun/2026), subpastas incluídas.
    let want: usize =
        std::env::var("SWOOP_TEST_MEDIAFIRE_FOLDER_COUNT").map_or(136, |v| v.parse().unwrap());
    assert_eq!(files.len(), want);
}

#[tokio::test]
#[ignore = "rede"]
async fn drive_pequeno_grande_e_removido() {
    let reg = registry();
    let small = link(
        "SWOOP_TEST_GDRIVE_SMALL",
        "https://drive.google.com/uc?id=1cKSdgtWrPgvEsBGmjWALOH33taGyVXKb",
    );
    let info = reg.check(&small).await.expect("check do Drive (pequeno)");
    println!("drive pequeno: {info:?}");
    assert_eq!(info.size, Some(5), "o arquivo do gdown tem \"spam\\n\"");

    // Grande: passa pela página "não dá para verificar vírus".
    let big = link(
        "SWOOP_TEST_GDRIVE_BIG",
        "https://drive.google.com/uc?id=1s52ek_4YTDRt_EOkx1FS53u-vJa0c4nu",
    );
    let r = resolve(&reg, big).await.expect("resolve do Drive (grande)");
    println!("drive grande: {:?} {:?}", r.file_name, r.size);
    assert!(r.size.unwrap_or(0) > 100 * 1024 * 1024);
    assert_offline_fast(
        &reg,
        "https://drive.google.com/file/d/1zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz/view",
    )
    .await;
}

#[tokio::test]
#[ignore = "rede"]
async fn drive_pasta() {
    let reg = registry();
    let url = link(
        "SWOOP_TEST_GDRIVE_FOLDER",
        "https://drive.google.com/drive/folders/15uNXeRBIhVvZJIhL4yTw4IsStMhUaaxl",
    );
    let files = reg
        .expand(&url)
        .await
        .unwrap()
        .expect("não abriu como pasta");
    println!("drive: pasta com {} arquivo(s)", files.len());
    assert!(!files.is_empty());
}
