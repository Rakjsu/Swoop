//! `swoop-cli`: o Swoop sem janela.
//!
//! Na fase 0 só responde `--version`/`info`. Os subcomandos `get`, `resume`,
//! `check`, `resolve`, `serve` e `extract` entram nas fases seguintes, sempre
//! sobre o mesmo motor usado pelo app desktop.

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "swoop-cli",
    version,
    about = "Gerenciador de downloads Swoop (modo sem janela)"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Mostra nome e versão
    Info,
}

/// Liga os logs em stderr; o nível vem de `RUST_LOG` (padrão `info`).
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

fn main() -> eyre::Result<()> {
    init_tracing();
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Info) {
        Command::Info => println!("{}", swoop_core::version_line()),
    }
    Ok(())
}
