//! Portão 3d: o link direto expira no meio do download → o motor resolve de
//! novo exatamente uma vez e continua do ponto em que estava.

mod common;

use async_trait::async_trait;
use common::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use swoop_core::{HostError, ResolveRequest, Resolved, Resolver};
use swoop_testsrv::{TestServer, effective_seed, sha256_hex};
use url::Url;

/// Plugin de teste: o 1º link vale meio segundo; os seguintes, dez minutos.
struct Expiring {
    base: String,
    calls: AtomicU32,
}

#[async_trait]
impl Resolver for Expiring {
    async fn resolve(&self, _req: &ResolveRequest) -> Result<Resolved, HostError> {
        let first = self.calls.fetch_add(1, Ordering::SeqCst) == 0;
        let life = if first { 500 } else { 600_000 };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let url = Url::parse(&format!("{}&valid_until={}", self.base, now + life)).unwrap();
        Ok(Resolved::direct(url))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn link_expirado_resolve_uma_vez_e_continua() {
    let srv = TestServer::start().await.unwrap();
    let size = 8 * MIB;
    // A conexão cai a cada 3 MiB (~1,5 s): a reconexão já pega o link vencido.
    let query = format!(
        "size={size}&seed=21&rate={}&drop_after={}",
        2 * MIB,
        3 * MIB
    );
    let resolver = Arc::new(Expiring {
        base: srv.file_url("expira.bin", &query),
        calls: AtomicU32::new(0),
    });
    let h = Harness::with_resolver(settings(1), resolver.clone()).await;
    let id = h
        .add("https://servidor.exemplo/arquivo/expira".into())
        .await;

    let row = &h.wait_final(&[id], Duration::from_secs(60)).await[0];
    assert_eq!(
        sha256_file(&completed(row)),
        sha256_hex(effective_seed(21, 0), size)
    );
    assert_eq!(
        resolver.calls.load(Ordering::SeqCst),
        2,
        "re-resolveu mais de uma vez"
    );
    // Continuou do ponto: além do arquivo, só o que estava em trânsito em
    // cada queda (um pedaço do servidor por queda). Recomeçar do zero
    // passaria de 3 MiB a mais.
    let sent = srv.stats().bytes_sent;
    assert!(
        sent < size + MIB,
        "baixou de novo o que já tinha ({sent} bytes)"
    );
}

/// Plugin de teste: entrega os links da lista em ordem (o último se repete).
struct Links {
    urls: Vec<String>,
    calls: AtomicU32,
}

#[async_trait]
impl Resolver for Links {
    async fn resolve(&self, _req: &ResolveRequest) -> Result<Resolved, HostError> {
        let i = self.calls.fetch_add(1, Ordering::SeqCst) as usize;
        let url = &self.urls[i.min(self.urls.len() - 1)];
        Ok(Resolved::direct(Url::parse(url).unwrap()))
    }
}

async fn run_links(urls: Vec<String>) -> (swoop_store::DownloadRow, u32, std::path::PathBuf) {
    let links = Arc::new(Links {
        urls,
        calls: AtomicU32::new(0),
    });
    let h = Harness::with_resolver(settings(2), links.clone()).await;
    let id = h.add("https://servidor.test/arquivo".into()).await;
    let row = h.wait_final(&[id], Duration::from_secs(20)).await.remove(0);
    (row, links.calls.load(Ordering::SeqCst), h.dest())
}

#[tokio::test(flavor = "multi_thread")]
async fn pagina_no_lugar_do_arquivo_resolve_de_novo_uma_vez() {
    let srv = TestServer::start().await.unwrap();
    let page = srv.file_url("erro.bin", "size=2048&ctype=text/html;%20charset=utf-8");
    let good = srv.file_url("bom.bin", "size=65536&seed=3");
    let (row, calls, _) = run_links(vec![page, good]).await;
    completed(&row);
    assert_eq!(calls, 2, "uma re-resolução");
}

#[tokio::test(flavor = "multi_thread")]
async fn pagina_sempre_falha_sem_gravar_nada() {
    let srv = TestServer::start().await.unwrap();
    let page = srv.file_url("erro.html", "size=2048&ctype=text/html");
    let (row, calls, dest) = run_links(vec![page]).await;
    assert_eq!(row.state, swoop_core::DownloadState::Failed);
    let msg = row.error_msg.unwrap_or_default();
    assert!(msg.contains("página em vez do arquivo"), "{msg}");
    assert_eq!(calls, 2);
    let files = std::fs::read_dir(&dest).map_or(0, |d| d.count());
    assert_eq!(files, 0, "nada gravado em {}", dest.display());
}

#[tokio::test(flavor = "multi_thread")]
async fn html_como_anexo_ainda_e_arquivo() {
    let srv = TestServer::start().await.unwrap();
    let file = srv.file_url("pagina.html", "size=4096&ctype=text/html&cd=1");
    let (row, calls, _) = run_links(vec![file]).await;
    completed(&row);
    assert_eq!(calls, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn erro_em_json_tambem_e_pagina() {
    // `{"error":"Denied by IP"}` no lugar do arquivo nunca vira o download.
    let srv = TestServer::start().await.unwrap();
    let json = srv.file_url("negado.bin", "size=64&ctype=application/json");
    let (row, calls, dest) = run_links(vec![json]).await;
    assert_eq!(row.state, swoop_core::DownloadState::Failed);
    assert_eq!(calls, 2);
    let files = std::fs::read_dir(&dest).map_or(0, |d| d.count());
    assert_eq!(files, 0, "nada gravado em {}", dest.display());
}

#[tokio::test(flavor = "multi_thread")]
async fn sem_range_o_link_e_pedido_uma_vez_so() {
    // Link de sessão (uso único): a resposta da sonda já é o download.
    let srv = TestServer::start().await.unwrap();
    let size = 3 * 1024 * 1024;
    let url = srv.file_url("unico.bin", &format!("size={size}&seed=8&norange=1"));
    let (row, calls, _) = run_links(vec![url]).await;
    completed(&row);
    assert_eq!(row.size, Some(size));
    assert_eq!(calls, 1);
    let stats = srv.stats();
    assert_eq!(stats.requests, 1, "o link foi pedido mais de uma vez");
    assert_eq!(stats.bytes_sent, size, "baixou mais que o arquivo");
}

#[tokio::test(flavor = "multi_thread")]
async fn arquivo_json_esperado_baixa_normal() {
    let srv = TestServer::start().await.unwrap();
    let json = srv.file_url("dados.json", "size=4096&ctype=application/json");
    let (row, calls, _) = run_links(vec![json]).await;
    completed(&row);
    assert_eq!(calls, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn sem_range_conexao_caida_e_pagina_na_volta_nao_vira_arquivo() {
    // A 1ª resposta cai no meio; o mesmo link (de uso único) devolve página
    // na volta. Nunca gravar a página como o arquivo, nem repetir o pedido
    // sem parar (antes: 1 pedido por segundo para sempre).
    let srv = TestServer::start().await.unwrap();
    let url = srv.file_url(
        "sessao.bin",
        &format!("size={}&norange=1&drop_after=65536&page_from=1", 1 << 20),
    );
    let links = Arc::new(Links {
        urls: vec![url],
        calls: AtomicU32::new(0),
    });
    let h = Harness::with_resolver(settings(2), links.clone()).await;
    let id = h.add("https://servidor.test/arquivo".into()).await;
    let t0 = std::time::Instant::now();
    while t0.elapsed() < Duration::from_secs(12) {
        let row = h.row(id).await;
        assert_ne!(
            row.state,
            swoop_core::DownloadState::Completed,
            "a página virou o arquivo"
        );
        if row.state == swoop_core::DownloadState::Failed {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let requests = srv.stats().requests;
    assert!(
        requests < 10,
        "pedidos demais no link de uso único: {requests}"
    );
}
