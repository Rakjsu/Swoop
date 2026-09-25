//! Build do instalador personalizado:
//! - embute o setup NSIS do Swoop indicado em `SWOOP_SETUP_PAYLOAD` (sem a
//!   variável, embute um arquivo vazio: build de desenvolvimento/CI que só
//!   mostra a janela e avisa que falta o pacote);
//! - no Windows, pede administrador (o Swoop instala em Arquivos de Programas
//!   e o setup silencioso herda a elevação);
//! - gera as permissões dos comandos da janela.

use std::path::{Path, PathBuf};

/// Manifesto do Windows: Common Controls 6 (padrão do Tauri) + administrador.
const MANIFEST: &str = r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>"#;

fn main() {
    embed_payload();
    let commands = tauri_build::AppManifest::new().commands(&[
        "installer_info",
        "choose_dir",
        "start_install",
        "launch_app",
    ]);
    let windows = tauri_build::WindowsAttributes::new()
        .app_manifest(MANIFEST)
        .window_icon_path("../swoop/icons/icon.ico");
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(commands)
            .windows_attributes(windows),
    )
    .expect("falha ao gerar o contexto do Tauri");
}

/// Copia o setup para `OUT_DIR/payload.exe` (lido com `include_bytes!`).
fn embed_payload() {
    println!("cargo:rerun-if-env-changed=SWOOP_SETUP_PAYLOAD");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("payload.exe");
    match std::env::var_os("SWOOP_SETUP_PAYLOAD").filter(|v| !v.is_empty()) {
        Some(src) => {
            let src = Path::new(&src);
            println!("cargo:rerun-if-changed={}", src.display());
            std::fs::copy(src, &out).unwrap_or_else(|e| {
                panic!("SWOOP_SETUP_PAYLOAD={}: {e}", src.display());
            });
        }
        None => std::fs::write(&out, []).expect("payload vazio"),
    }
}
