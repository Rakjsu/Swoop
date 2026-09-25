//! `swoop-cli`: o Swoop sem janela, sobre o mesmo motor do app desktop.
//!
//! - `get <links>…`: adiciona e baixa até terminar (retoma pendentes também);
//! - `resume`: continua o que ficou pela metade;
//! - `list`: mostra a fila;
//! - `serve`: painel no navegador (127.0.0.1) sobre o motor real;
//! - `check`: confere links (online, nome, tamanho) sem baixar;
//! - `resolve`: mostra o link direto e grava as respostas como fixtures;
//! - `info`: nome e versão.
//!
//! `extract` chega na fase 5.

mod hosts;
mod run;
mod serve;

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use swoop_core::Settings;
use swoop_core::units::parse_bytes;
use swoop_service::{Service, ServiceOptions};
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
    /// Baixa os links (e continua os pendentes da fila)
    Get {
        /// Links http(s)
        #[arg(required = true)]
        links: Vec<String>,
        /// Pasta de destino (padrão: Downloads/Swoop)
        #[arg(long)]
        dest: Option<PathBuf>,
        #[command(flatten)]
        common: Common,
    },
    /// Continua os downloads interrompidos
    Resume {
        #[command(flatten)]
        common: Common,
    },
    /// Lista a fila
    List {
        #[command(flatten)]
        common: Common,
    },
    /// Confere links sem baixar (aceita texto com links no meio)
    Check {
        /// Links ou texto com links
        #[arg(required = true)]
        texts: Vec<String>,
        /// Pasta de dados (regras do usuário em rules/hosts.toml)
        #[arg(long)]
        data_dir: Option<PathBuf>,
    },
    /// Mostra o link direto de um arquivo
    Resolve {
        /// Link do arquivo
        link: String,
        /// Grava cada resposta lida nesta pasta (para virar fixture)
        #[arg(long)]
        dump_fixtures: Option<PathBuf>,
        /// Pasta de dados (regras do usuário em rules/hosts.toml)
        #[arg(long)]
        data_dir: Option<PathBuf>,
    },
    /// Painel no navegador em 127.0.0.1 (imprime o endereço com o token)
    Serve {
        /// Pasta com o build da UI
        #[arg(long, default_value = "ui/dist")]
        ui: PathBuf,
        /// Porta (0 = qualquer livre)
        #[arg(long, default_value_t = 0)]
        port: u16,
        /// Pasta de dados (banco); padrão: SWOOP_DATA_DIR ou a pasta do SO
        #[arg(long)]
        data_dir: Option<PathBuf>,
    },
}

/// Opções comuns aos comandos que abrem o motor.
#[derive(Args)]
struct Common {
    /// Pasta de dados (banco); padrão: SWOOP_DATA_DIR ou a pasta do SO
    #[arg(long)]
    data_dir: Option<PathBuf>,
    /// Conexões por arquivo
    #[arg(long, default_value_t = Settings::default().connections_per_download)]
    connections: u16,
    /// Downloads ao mesmo tempo
    #[arg(long, default_value_t = Settings::default().max_active_downloads)]
    parallel: usize,
    /// Limite global de velocidade (ex.: 512K, 2M)
    #[arg(long, value_parser = parse_bytes)]
    limit: Option<u64>,
}

impl Common {
    /// Opções do serviço a partir dos argumentos.
    fn options(&self) -> ServiceOptions {
        ServiceOptions {
            data_dir: self.data_dir.clone(),
            settings: Some(Settings {
                connections_per_download: self.connections.max(1),
                max_active_downloads: self.parallel.max(1),
                speed_limit_bps: self.limit,
                ..Settings::default()
            }),
        }
    }
}

/// Liga os logs em stderr; o nível vem de `RUST_LOG` (padrão `info`).
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    init_tracing();
    let cli = Cli::parse();
    let code = match cli.command.unwrap_or(Command::Info) {
        Command::Info => {
            println!("{}", swoop_core::version_line());
            0
        }
        Command::Get {
            links,
            dest,
            common,
        } => {
            let service = Service::open(common.options()).await?;
            service
                .add_links(&links, dest.as_deref(), "linha de comando")
                .await?;
            run::until_idle(service).await?
        }
        Command::Resume { common } => {
            let service = Service::open(common.options()).await?;
            run::until_idle(service).await?
        }
        Command::Serve { ui, port, data_dir } => serve::serve(data_dir, ui, port).await?,
        Command::Check { texts, data_dir } => hosts::check(&texts, data_dir).await?,
        Command::Resolve {
            link,
            dump_fixtures,
            data_dir,
        } => hosts::resolve(&link, dump_fixtures, data_dir).await?,
        Command::List { common } => {
            let service = Service::open(common.options()).await?;
            run::print_list(&service.rows().await?);
            service.shutdown().await;
            0
        }
    };
    std::process::exit(code);
}
