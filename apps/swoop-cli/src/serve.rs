//! `swoop-cli serve`: o motor com o painel no navegador, só em 127.0.0.1.
//! Imprime o endereço (com o token no `#`) numa linha do stdout e roda até
//! o Ctrl+C, gravando o progresso ao sair.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use swoop_remote::RemoteOptions;
use swoop_service::{Service, ServiceOptions};

/// Sobe serviço + painel e espera o Ctrl+C.
pub async fn serve(data_dir: Option<PathBuf>, ui: PathBuf, port: u16) -> eyre::Result<i32> {
    if !ui.join("index.html").exists() {
        eyre::bail!(
            "não achei {}; rode `npm run build` em ui/ ou passe --ui",
            ui.join("index.html").display()
        );
    }
    // Preferências salvas: as mesmas que a tela de Opções grava.
    let service = Arc::new(
        Service::open(ServiceOptions {
            data_dir,
            settings: None,
        })
        .await?,
    );
    let running = swoop_remote::start(
        service.clone(),
        RemoteOptions {
            port,
            ui_dir: Some(ui),
        },
    )
    .await?;
    let mut out = std::io::stdout().lock();
    writeln!(out, "{}", running.url())?;
    out.flush()?;
    drop(out);
    eprintln!("painel no ar; Ctrl+C para sair");

    tokio::signal::ctrl_c().await?;
    eprintln!("encerrando: gravando o progresso…");
    running.stop().await;
    service.shutdown().await;
    Ok(0)
}
