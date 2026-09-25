//! Execução em primeiro plano: mostra o progresso até a fila esvaziar e
//! encerra limpo no Ctrl+C (a escritora grava o último checkpoint).

use std::collections::HashMap;
use std::time::Duration;
use swoop_core::DownloadState;
use swoop_core::units::format_bytes;
use swoop_engine::Snapshot;
use swoop_service::Service;
use swoop_store::DownloadRow;

/// Intervalo entre linhas de progresso.
const PRINT_EVERY: Duration = Duration::from_secs(1);

/// Roda até não haver mais nada pendente; devolve o código de saída
/// (0 = tudo concluído, 1 = alguma falha, 130 = interrompido).
pub async fn until_idle(service: Service) -> eyre::Result<i32> {
    let snapshots = service.engine().snapshots();
    let mut tick = tokio::time::interval(PRINT_EVERY);
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                eprintln!("interrompido: gravando o progresso…");
                service.shutdown().await;
                return Ok(130);
            }
            _ = tick.tick() => {}
        }
        let rows = service.list().await?;
        print_progress(&snapshots.borrow(), &rows);
        if service.pending().await? == 0 && service.engine().active_count() == 0 {
            let failed = summarize(&rows);
            service.shutdown().await;
            return Ok(if failed { 1 } else { 0 });
        }
    }
}

/// Uma linha por download ativo.
fn print_progress(snap: &Snapshot, rows: &[DownloadRow]) {
    let names: HashMap<_, _> = rows.iter().map(|r| (r.id, r)).collect();
    for live in &snap.active {
        let name = names
            .get(&live.id)
            .and_then(|r| r.file_name.as_deref())
            .unwrap_or("…");
        let pct = live
            .total
            .map(|t| format!("{:5.1}%", live.received as f64 * 100.0 / t.max(1) as f64))
            .unwrap_or_else(|| format_bytes(live.received));
        println!(
            "{} {name}  {pct}  {}/s  {} conexões",
            live.id,
            format_bytes(live.speed_bps),
            live.conns
        );
    }
}

/// Resumo final; devolve se algo falhou.
fn summarize(rows: &[DownloadRow]) -> bool {
    let mut failed = false;
    for r in rows {
        match r.state {
            DownloadState::Completed => println!(
                "concluído {} → {}",
                r.id,
                r.final_path
                    .as_deref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            ),
            DownloadState::Failed => {
                failed = true;
                println!(
                    "falhou {} {}: {}",
                    r.id,
                    r.url,
                    r.error_msg.as_deref().unwrap_or("?")
                );
            }
            _ => {}
        }
    }
    failed
}

/// Tabela simples da fila.
pub fn print_list(rows: &[DownloadRow]) {
    if rows.is_empty() {
        println!("fila vazia");
    }
    for r in rows {
        let size = r.size.map(format_bytes).unwrap_or_else(|| "?".into());
        println!(
            "{:>6} {:<12} {:>10}  {}",
            r.id.to_string(),
            r.state.as_str(),
            size,
            r.file_name.as_deref().unwrap_or(&r.url)
        );
    }
}
