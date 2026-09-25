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
- **Tipos da UI:** `cargo test -p swoop-api` regenera `ui/src/gen` (ts-rs; `TS_RS_EXPORT_DIR`/`TS_RS_LARGE_INT` em `.cargo/config.toml`). Mudou DTO no `crates/api`: rodar e commitar; o CI falha se ficar diferente.
- **App com downloads de verdade sem Windows:** `cargo build -p swoop --features tauri/custom-protocol` (embute `ui/dist`), `Xvfb :55 &`, `DISPLAY=:55 SWOOP_DATA_DIR=… SWOOP_NO_UPDATE=1 ./target/debug/swoop`, testsrv em outra porta; cliques com `xdotool`, print com `import -window root`.
- **CLI:** `cargo run --release -p swoop-cli -- get <url> --connections 8 --limit 2M --data-dir data/dev --dest data/dl` · `resume` · `list`
- **Servidor de teste:** `cargo run --release -p swoop-testsrv -- 8765` (imprime URL e sha256 esperado; botões em `crates/testsrv/src/knobs.rs`)
- **Painel no navegador:** `cargo run -p swoop-cli -- serve --ui ui/dist` imprime `http://127.0.0.1:P/#t=<token>` (só loopback; token novo a cada execução).
- **Playwright (portão a da fase 2):** `cargo build -p swoop-cli -p swoop-testsrv && cd ui && npm run e2e`. Na nuvem, `PW_CHROMIUM=/opt/pw-browsers/chromium-1194/chrome-linux/chrome` (não rodar `playwright install`); no CI, `npx playwright install --with-deps chromium`.
- **Portão b da fase 2:** `cargo test -p swoop-remote --test http websocket -- --nocapture` (45–55 retratos em 10 s pelo WebSocket, nenhum outro aviso).
- **Portões da fase 1:**
  - motor, dentro do processo (b, c, d, f): `cargo test -p swoop-engine --test download -- --nocapture`
  - conexão lenta (e), num binário só dele por medir tempo: `cargo test -p swoop-engine --test slow_connection -- --nocapture`
  - matar e retomar (a): `cargo test -p swoop-cli --test kill_resume -- --nocapture`
  - versão de 1 GiB: `cargo test --release -p swoop-cli --test kill_resume -- --ignored --nocapture`
- **Servidores (fase 3):**
  - conferir/resolver sem banco: `cargo run -p swoop-cli -- check <links ou texto>` · `resolve <link> --dump-fixtures <pasta>` (grava só corpos, nunca cabeçalhos; revisar antes de commitar como fixture)
  - portão (d), link expirando: `cargo test -p swoop-engine --test reresolve -- --nocapture`
  - rede (links públicos; trocáveis por `SWOOP_TEST_*`): `cargo test -p swoop-hosts --test network -- --ignored --test-threads=1 --nocapture` e `cargo test -p swoop-engine --test network -- --ignored --test-threads=1 --nocapture`. Na nuvem o proxy bloqueia os sites até o dono liberar os domínios.
- **Checagem para Windows sem Windows:** `rustup target add x86_64-pc-windows-gnu`, `apt install gcc-mingw-w64-x86-64`, depois `cargo clippy --target x86_64-pc-windows-gnu --all-targets -- -D warnings`
- **Linux sem display (nuvem):** `xvfb-run -a ./target/release/swoop`. Print com `import -window root x.png`.

## Publicar versão
- **Publicar versão:** subir `version` em `[workspace.package]` do `Cargo.toml` e fazer o merge na `main`. Depois, em **Actions → Release → Run workflow**, o workflow cria a tag `v<versão>` e publica a release. Pela API, é o `workflow_dispatch` do `release.yml`. O proxy da sessão de nuvem recusa push de tag (403); da máquina do dono, `git push` da tag também funciona.
- **Contrato com `crates/update`, não renomear:** `Swoop_<versão>_x64-setup.exe` e `SHA256SUMS.txt`. A atualização só aceita URLs de `github.com/Rakjsu/Swoop/releases/download/`.
- **Parâmetros do setup NSIS:**
  - o atualizador roda `/P /UPDATE /R` (passivo, sem recriar atalhos, reabre o app como usuário comum);
  - o instalador personalizado roda `/S /D=<pasta>`, com `/D=` por último e sem aspas.
- **Atualização em dev:** fica desligada; liga com `SWOOP_UPDATE_CHECK=1`, e `SWOOP_NO_UPDATE=1` desliga em qualquer build.
- **Instalador personalizado sem `SWOOP_SETUP_PAYLOAD`:** abre a janela, mas recusa instalar ("build de desenvolvimento").

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
- **Comando Tauri:** argumentos chegam em camelCase do JS (`subscribe({ onPush })`). Comando async com `State` devolve `Result` (erro = `ApiError`, serializável).
- **Assinatura da UI (`subscribe`):** o Rust guarda só uma (a nova derruba a anterior). A UI assina uma vez por carga, no módulo `ui/src/transport/tauri.ts`; componentes ouvem o emissor local. Assinar num `useEffect` quebraria no StrictMode.
- **Pasta automática:** pacote com `auto_dest` escolhe a pasta pelo tipo (`core::file_kind`) no `prepare`, quando o nome real já é conhecido; retomada termina na pasta do `.part`. Pasta escolhida à mão vale para o pacote inteiro.
- **Preferências:** `ServiceOptions.settings = None` usa as salvas (app, `serve`); `Some` (CLI) não grava. Pastas `None` são preenchidas com as do sistema + `Swoop` antes de chegar ao motor.
- **Painel (`crates/remote`):** guarda de `Host`/`Origin` em tudo; API com `Bearer`; WebSocket autentica pela 1ª mensagem (navegador não manda cabeçalho). Token nunca vai para log (`Debug` do `Token` esconde).
- **Remover download:** `Engine::remove` marca o id em `removing` (sob a trava do `active`, que o `spawn_job` confere) antes de parar o job; só depois grava histórico + DELETE e apaga o `.part`. O `exec` responde na hora e a remoção termina em segundo plano.
- **Estado de download:** muda só via `downloads::transition` (valida com `DownloadState::next`). Transição recusada no job significa que o usuário pausou ou removeu, e o job sai quieto (`TransferError::Cancelled`).
- **Checkpoint:** a cada 500 ms ou 8 MiB, sempre `sync_data` antes do SQLite. O banco nunca afirma bytes que não estão no disco.
- **Testes com `TestServer`:** usar `#[tokio::test(flavor = "multi_thread")]`. Para medir conexões simultâneas, limitar a taxa (`rate=`), senão no loopback uma conexão termina antes da outra abrir.
- **`tsc`:** TypeScript 7 (nativo) ainda não é suportado pelo typescript-eslint; manter `~6.0`.
- **Plugins (`crates/hosts`):** `<servidor>/parse.rs` é puro e testado com fixtures em `crates/hosts/tests/fixtures/<servidor>/`; `mod.rs` só faz rede. Seletor, endereço ou texto de página vai em `rules/hosts.toml` (o usuário sobrescreve em `<dados>/rules/hosts.toml`), nunca fixo no código.
- **Leitura de página:** sempre por `page::get`/`page::read_body` — detecta quando o "link da página" já entrega o arquivo (o Mediafire faz isso com alguns links) e corta em 4 MiB. `res.text()` direto baixaria o arquivo inteiro para a memória.
- **Erro de rede do reqwest:** `page::network` usa `without_url()` e só as causas internas; o `Display` do próprio erro traz a URL.
- **`HostError::BrowserRequired`:** a mensagem é mostrada como está, então diz o servidor e o que fazer ("o Mediafire pediu captcha; abra o link no navegador").
- **Coletor (`crates/service/src/collector.rs` + `crates/store/src/collector.rs`):** URL única (colar de novo não duplica); `batch` = uma colagem (vira um pacote no "iniciar sozinho"); verificação em levas de até 4, uma por servidor (`host_key` = plugin ou domínio); `checking` volta para `unchecked` ao reabrir. A janela usa `Command::Collect`; `AddLinks` direto fica para CLI/API.
- **Lista de Downloads com pacotes:** cabeçalho e linhas têm a mesma altura (`ROW_HEIGHT`), achatados por `buildItems` — a lista virtual continua simples. Recolhidos ficam no `localStorage` (try/catch).
- **Pasta do Drive:** a página `embeddedfolderview` é lida por todos os `<a href>` (como o gdown), sem depender de classe; Docs/Planilhas nativos são ignorados.

## Nunca
- Burlar espera, cota ou captcha de servidor, ou trocar IP para fugir de limite. Isso inclui o "baixar assim mesmo" do aviso de arquivo perigoso do Mediafire: a decisão é do usuário, no navegador.
- Commitar `.env`, token, chave de API ou senha de conta premium.
- Guardar segredo de conta no SQLite (vai para o cofre do SO via `keyring`).
