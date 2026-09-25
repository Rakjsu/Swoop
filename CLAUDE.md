# swoop — instruções locais

Workspace Cargo (Rust 2024) com Tauri 2.11 e UI React + TypeScript + Vite em `ui/`. O plano de
fases com portões está em `docs/specs/2026-09-25-swoop-design.md`: ler antes de mexer.

## Comandos
- **Build, lint e testes (crates e CLI):** `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test`
- **UI:** `cd ui && npm run lint && npm run build`
- **App desktop:**
  - lint: `cargo clippy -p swoop --all-targets -- -D warnings`. Precisa de `ui/dist`, então buildar a UI antes; `apps/swoop` fica fora de `default-members` justamente por isso.
  - dev: `cd apps/swoop && ../../ui/node_modules/.bin/tauri dev --config tauri.dev.conf.json`. O identificador `.dev` separa dados e instância única do app instalado.
  - release sem instalador: `cd apps/swoop && ../../ui/node_modules/.bin/tauri build --no-bundle`
- **CLI:** `cargo run --release -p swoop-cli -- get <url> --connections 8 --limit 2M --data-dir data/dev --dest data/dl` · `resume` · `list`
- **Servidor de teste:** `cargo run --release -p swoop-testsrv -- 8765` (imprime URL e sha256 esperado; botões em `crates/testsrv/src/knobs.rs`)
- **Portões da fase 1:**
  - motor, dentro do processo (b–f): `cargo test -p swoop-engine --test download -- --nocapture`
  - matar e retomar (a): `cargo test -p swoop-cli --test kill_resume -- --nocapture`
  - versão de 1 GiB: `cargo test --release -p swoop-cli --test kill_resume -- --ignored --nocapture`
- **Checagem para Windows sem Windows:** `rustup target add x86_64-pc-windows-gnu`, `apt install gcc-mingw-w64-x86-64`, depois `cargo clippy --target x86_64-pc-windows-gnu --all-targets -- -D warnings`
- **Linux sem display (nuvem):** `xvfb-run -a ./target/release/swoop`. Print com `import -window root x.png`.

## Convenções
- Código e comentários em português, identificadores em inglês.
- Cada crate tem UMA responsabilidade e testa sozinho. Parsers de servidor são puros (`parse.rs`) e testados com fixtures; o I/O fica em `mod.rs`.
- Toda fase termina num portão medido: número ou comparação objetiva, não "funciona".
- Dados simulados só em testes. Não existe modo demo: a UI em dev fala com o motor real.
- **Comando Tauri novo** entra em três lugares: `generate_handler!` (`src/main.rs`), `AppManifest::commands` (`build.rs`) e `capabilities/main.json` (`allow-<nome>`).
- **Janelas de captcha (fase 4)** nunca recebem capability. Página de servidor não fala com o Rust.
- **TLS:** rustls com **ring**. `aws-lc-sys` na árvore exige NASM no Windows, e o CI falha se ele aparecer.
- **Commits:** mensagens em português, no estilo `feat: ...`/`fix: ...`, sem `Co-Authored-By`.

## Armadilhas
- `generate_context!` exige `ui/dist` existente em tempo de compilação.
- `cookies_for_url` do WebView trava no Windows se for chamado de comando síncrono: usar só em contexto async.
- **Escrita em disco do motor:** nunca `tokio::fs` com seek + write, porque o cursor é compartilhado. Usar escrita posicional (`write_at`/`seek_write`).
- **Segmentos HTTP:** usar `http1_only` e `Accept-Encoding: identity`. HTTP/2 junta as conexões numa só; gzip quebra os offsets.
- **Logs:** nunca logar URL crua (a chave do Mega fica no `#`), nem `Authorization` ou `Cookie`.
- **Pasta Música no Windows** (`C:\Users\...\Music`): o WMPNetworkSvc trava pastas novas e o rename do build falha com EPERM (visto no NeoStream). Builds de instalador vão para uma pasta temporária fora dela.
- **Migrações do SQLite:** ficam em `crates/store/src/migrations/` e a versão vai em `PRAGMA user_version`. Migração nova é arquivo novo no fim da lista, nunca edição de uma já publicada. O `rusqlite_migration` foi descartado porque exige rustc 1.95.
- **Estado de download:** muda só via `downloads::transition` (valida com `DownloadState::next`). Transição recusada no job significa que o usuário pausou ou removeu, e o job sai quieto (`TransferError::Cancelled`).
- **Checkpoint:** a cada 500 ms ou 8 MiB, sempre `sync_data` antes do SQLite. O banco nunca afirma bytes que não estão no disco.
- **Testes com `TestServer`:** usar `#[tokio::test(flavor = "multi_thread")]`. Para medir conexões simultâneas, limitar a taxa (`rate=`), senão no loopback uma conexão termina antes da outra abrir.
- **`tsc`:** TypeScript 7 (nativo) ainda não é suportado pelo typescript-eslint; manter `~6.0`.

## Nunca
- Burlar espera, cota ou captcha de servidor, ou trocar IP para fugir de limite.
- Commitar `.env`, token, chave de API ou senha de conta premium.
- Guardar segredo de conta no SQLite (vai para o cofre do SO via `keyring`).
