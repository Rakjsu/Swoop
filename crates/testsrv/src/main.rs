//! `swoop-testsrv [porta]`: sobe o servidor de teste para ensaios manuais
//! (ex.: baixar com o app no Windows um arquivo de tamanho e sha256 conhecidos).

use std::net::SocketAddr;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(8765);
    let server = swoop_testsrv::TestServer::bind(SocketAddr::from(([127, 0, 0, 1], port))).await?;
    let size: u64 = 256 << 20;
    println!("servidor de teste em http://{}", server.addr());
    println!(
        "exemplo: {}",
        server.file_url("teste.bin", &format!("size={size}&seed=1"))
    );
    println!(
        "sha256 esperado: {}",
        swoop_testsrv::sha256_hex(swoop_testsrv::effective_seed(1, 0), size)
    );
    tokio::signal::ctrl_c().await?;
    Ok(())
}
