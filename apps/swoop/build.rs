//! Gera o contexto do Tauri e as permissões dos comandos do app.
//!
//! Cada comando listado em `AppManifest::commands` ganha a permissão
//! `allow-<comando>`; só a janela principal recebe essas permissões
//! (capabilities/main.json). Comando novo precisa entrar nas duas listas.

fn main() {
    let manifest =
        tauri_build::AppManifest::new().commands(&["app_info", "update_status", "install_update"]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("falha ao gerar o contexto do Tauri");
}
