//! Comandos dos servidores, sem abrir o banco nem o motor:
//!
//! - `check`: nome, tamanho e se está online (pastas viram os arquivos);
//! - `resolve`: o link direto que o motor usaria, gravando opcionalmente as
//!   respostas lidas como fixtures.
//!
//! Links aparecem redigidos (sem query nem `#`), como nos logs.

use std::path::PathBuf;
use std::time::SystemTime;
use swoop_core::units::format_bytes;
use swoop_core::{HostError, ResolveRequest, Resolver};
use swoop_hosts::{Registry, Rules};
use swoop_net::Redacted;
use url::Url;

/// Registro com as regras da pasta de dados (e gravação, se pedida).
fn registry(data_dir: Option<PathBuf>, dump: Option<PathBuf>) -> eyre::Result<Registry> {
    let data_dir = data_dir.unwrap_or_else(swoop_service::default_data_dir);
    let registry = Registry::new(Rules::for_data_dir(&data_dir))?;
    Ok(match dump {
        Some(dir) => registry.dump_to(dir),
        None => registry,
    })
}

/// Confere cada link; 0 = todos online, 1 = algum não.
pub async fn check(texts: &[String], data_dir: Option<PathBuf>) -> eyre::Result<i32> {
    let reg = registry(data_dir, None)?;
    let links = reg.detect(&texts.join("\n"));
    if links.is_empty() {
        eprintln!("nenhum link encontrado");
        return Ok(1);
    }
    let mut all_ok = true;
    for link in links {
        let files = match reg.expand(&link).await {
            Ok(Some(files)) => {
                println!("pasta {}: {} arquivo(s)", Redacted(&link), files.len());
                files
            }
            Ok(None) => vec![link],
            Err(e) => {
                println!(
                    "{:<10} {}  {}",
                    reg.host_of(&link),
                    describe(&e),
                    Redacted(&link)
                );
                all_ok = false;
                continue;
            }
        };
        for file in files {
            all_ok &= check_one(&reg, &file).await;
        }
    }
    Ok(if all_ok { 0 } else { 1 })
}

/// Uma linha por arquivo; devolve se está online.
async fn check_one(reg: &Registry, url: &Url) -> bool {
    let host = reg.host_of(url);
    match reg.check(url).await {
        Ok(info) => {
            let size = info.size.map(format_bytes).unwrap_or_else(|| "?".into());
            let name = info.name.as_deref().unwrap_or("?");
            println!("{host:<10} online  {size:>10}  {name}");
            true
        }
        Err(e) => {
            println!("{host:<10} {}  {}", describe(&e), Redacted(url));
            false
        }
    }
}

/// Resolve um link e mostra o que o motor receberia.
pub async fn resolve(
    link: &str,
    dump: Option<PathBuf>,
    data_dir: Option<PathBuf>,
) -> eyre::Result<i32> {
    let url = Url::parse(link.trim()).map_err(|_| eyre::eyre!("link inválido"))?;
    let reg = registry(data_dir, dump)?;
    let req = ResolveRequest::new(url);
    match reg.resolve(&req).await {
        Ok(r) => {
            println!("servidor:  {}", r.host_key);
            println!("nome:      {}", r.file_name.as_deref().unwrap_or("?"));
            println!(
                "tamanho:   {}",
                r.size.map(format_bytes).unwrap_or_else(|| "?".into())
            );
            println!("conexões:  {}", r.max_connections);
            println!(
                "integridade: {}",
                if r.integrity.is_some() { "sim" } else { "não" }
            );
            println!("link final: {}", Redacted(&r.url));
            Ok(0)
        }
        Err(e) => {
            println!("{}", describe(&e));
            Ok(1)
        }
    }
}

/// Erro em pt-BR; espera mostra quanto falta.
fn describe(e: &HostError) -> String {
    match e {
        HostError::Offline => "offline (removido ou inexistente)".into(),
        HostError::Wait { until, reason } => {
            let secs = until
                .duration_since(SystemTime::now())
                .map_or(0, |d| d.as_secs());
            format!(
                "aguardando ({}; ~{} min)",
                reason.describe(),
                secs.div_ceil(60)
            )
        }
        e => format!("erro: {e}"),
    }
}
