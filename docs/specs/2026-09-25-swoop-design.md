# Swoop — desenho e plano de fases

Data: 2026-09-25. Documento de referência: mudanças de rumo entram aqui com data.

## Contexto
O dono quer um app desktop parecido com o Mipony. Ele cola ou captura links de servidores de
hospedagem e o app:
- descobre o link real (resolvendo captcha e espera junto com o usuário);
- baixa em fila, com várias conexões por arquivo;
- descompacta ao terminar;
- pode ser acompanhado pelo celular.

O repositório atual (mev-bot) não tem relação com isso. **Nada muda no mev-bot**: o app nasce
num repositório novo e segue as mesmas convenções:
- workspace Cargo com um crate por responsabilidade;
- comentários em português;
- toda fase termina num portão medido;
- segredos só no `.env`.

## Decisões do dono
- **Nome:** Swoop. Repo privado `rakjsu/swoop` (eu crio e anexo à sessão), app `swoop.exe`, CLI `swoop-cli`, crates `swoop-*`.
- **Stack:** Rust + Tauri 2.11.x (a linha 3.0 ainda é alpha e fica de fora), com UI em React + TypeScript + Vite.
- **MVP:**
  - motor de download: fila, downloads simultâneos, conexões por arquivo, pausar e retomar que sobrevive a reinício, limite global de velocidade;
  - plugins de servidores;
  - captura de links: área de transferência e extensão de navegador;
  - extração automática com lista de senhas;
  - controle remoto pelo celular.
- **Servidores:** fastfile.cc, Mega, Mediafire, Google Drive, Gofile e Pixeldrain.
- **Gofile:** modo visitante, com o "sal" num arquivo de regras atualizável sem recompilar, marcado como experimental. Se o usuário tiver token premium, o app usa esse token.
- **Postura:**
  - nada de contornar limites: sem reconexão de roteador, sem burlar espera ou cota, sem serviço pago de captcha;
  - o captcha é resolvido pelo usuário;
  - as esperas aparecem com contagem regressiva;
  - as contas premium são do próprio usuário.

## Achados que moldam o desenho
- **fastfile.cc** usa XFileSharing Pro.
  - Fluxo grátis: formulário `download1` → `download2`, com contador e reCAPTCHA v2.
  - Grátis tem 1 conexão e não retoma. Premium tem 5 conexões e retoma.
  - A API XFS precisa de chave (`/api/file/direct_link`).
  - O proxy da nuvem bloqueia o site, então o HTML ao vivo não foi inspecionado.
  - Por isso: plugin **XFS genérico**, com o fluxo grátis feito numa janela WebView que intercepta o download (`on_download`).
- **Mega:** AES-128-CTR com seek, o que permite segmentar e retomar. Range vai por sufixo `URL/ini-fim`. O MAC é conferido num passe final. Há hashcash quando a API responde 402, e cota via 509 + `X-MEGA-Time-Left`. O crate `mega` está parado desde 2024, então a implementação é própria (`aes`+`ctr`+`cbc`+`sha2`).
- **Mediafire:** API `get_info` mais `#downloadButton` / `data-scrambled-url`.
- **Google Drive:** `drive.usercontent.google.com/download?...&confirm=t` e `#download-form`. Pastas via `embeddedfolderview`.
- **Pixeldrain:** API oficial `/api/file/{id}` com Range. O gratuito tem 6 GB/24 h e usa 1 conexão.
- **Gofile:** `X-Website-Token = sha256(UA::lang::token::floor(t/14400)::salt)`.
- **Bibliotecas:**
  - `reqwest` 0.13 com rustls sobre **ring**, que evita exigir NASM no Windows;
  - `rusqlite` 0.40 (bundled) + `rusqlite_migration`;
  - `async-speed-limit`, `fs4`, `positioned-io`;
  - `axum` 0.8, `ts-rs`, `clipboard-master` (o plugin oficial não avisa quando o clipboard muda), `keyring`;
  - **7-Zip 26.03 como sidecar** (`unrar` tem CVE e trava em multiparte no Windows; libarchive não abre RAR com senha).
- **Não usar:** `tauri-plugin-sql`/`sqlx` (conflito de `libsqlite3-sys`), `tauri-plugin-http` (puxa um segundo reqwest) e crates prontos de download (nenhum cobre segmentos + limite global + persistência).

## Layout do repositório `rakjsu/swoop`
```
Cargo.toml              workspace (resolver 3, edition 2024, rust-version 1.93); default-members sem apps/swoop
.gitignore .editorconfig .gitattributes (eol=lf; fixtures -text) .env.example (só nomes SWOOP_TEST_*)
README.md CLAUDE.md docs/specs/2026-09-25-swoop-design.md docs/remoto.md
crates/core      IDs, máquina de estados pura, Resolved, erros, traits Resolver/ContentCodec/Verifier/BrowserBridge,
                 Settings, sanitize_component/safe_join (sem I/O)
crates/api       DTOs (ts-rs), Command/Reply/Snapshot/Event, trait Backend (sem I/O)
crates/store     SQLite numa thread-ator, WAL, migrações, repositórios
crates/net       fábrica de reqwest (http1_only nos segmentos, UA, cookies por perfil), parsers de
                 Content-Range/Content-Disposition, Redacted<Url>
crates/engine    agendador, sonda, plano de segmentos (puro), workers, escritor, limitador, progresso, verificação
crates/hosts     HostPlugin, Registry (implementa Resolver), detect, regras; plugins em <host>/{mod.rs (I/O), parse.rs (puro)}
crates/extract   trait Extractor + 7-Zip (processo), volumes, ordem de senhas, validação de caminhos
crates/service   liga tudo; pós-processo; histórico; contas (keyring); implementa api::Backend
crates/remote    axum: painel + WS + auth por token + guarda de Host/Origin + porta da extensão (depende só de api)
crates/testsrv   servidor de teste com Range e botões de falha + replay de fixtures (publish = false)
apps/swoop       Tauri (commands exec/list/subscribe via Channel, bandeja, janela de captcha, clipboard)
apps/swoop-cli   headless: get | resume | check | resolve --dump-fixtures | serve | extract
ui/              React+TS; src/transport/{tauri,http}.ts (isTauri → invoke/Channel; senão fetch+WS); src/gen (ts-rs)
extension/       MV3 (menu de contexto → POST 127.0.0.1 com token)
rules/hosts.toml UA, seletores, regex, sal do Gofile, lista de bloqueio de anúncios (só dados)
scripts/         fetch-7z.{ps1,sh} (versão e sha256 fixos), net-check.ps1
```
Não há ciclos entre os crates:
- `engine` recebe `Arc<dyn Resolver>` e não conhece `hosts`.
- `remote` recebe `Arc<dyn Backend>` e não conhece `service`.
- `hosts` não conhece `store`.

## Contratos centrais (resumo)
- **`Resolved`:** `url`, `headers`, `cookies`, `file_name?`, `size?`, `integrity?` (Sha256 | Md5 | Custom), `range` (Probe | None | Header | PathSuffix), `max_connections`, `resumable`, `expires_at?`, `host_key`, `codec?` (Mega).
  - Nunca é persistido: a retomada sempre resolve de novo.
- **`HostPlugin`:** `id`, `matches(url)` (puro), `limits(conta)`, `check`, `expand` (pastas), `resolve(prev)` (re-resolve quando `prev` existe), `classify(falha HTTP)` → `ErrorClass` (Retry | Reresolve | Wait{until} | BrowserRequired | Fatal), `account_info`.
  - Cada plugin recebe um `HostCtx` com `http`, `rules`, `browser`, `account`, `status` (Countdown | Captcha | ProofOfWork) e `cancel`.
- **`BrowserBridge::capture_download(page_url, profile, UA, block_domains, timeout)`** → `{final_url, cookies, referer}`. É implementado pela janela WebView do Tauri.
- **`Extractor`:** `list`, `test(senha)` e `extract(dest, senha, progresso, cancel)`.
- **`api::Backend`:** `exec(Caller, Command)`, `list`, `snapshots()` (watch a 4–5 Hz) e `events()` (broadcast de eventos discretos).
  - `Caller::Panel` não mexe em contas, senhas nem remoto.
  - `Caller::Extension` só pode usar `AddLinks`.
- **Pipeline:**
  1. `resolve` → sonda `Range: bytes=0-0` (com `Accept-Encoding: identity`) → plano de segmentos → N workers.
  2. Cada worker faz `GET Range` + `If-Range`, passa pelo `limiter.consume` e pelo `codec.decode(offset)`, e entrega a um canal limitado.
  3. Uma thread escritora grava com `write_at`/`seek_write`. A cada 1 s ou 16 MiB faz `sync_data()` e em seguida o checkpoint no SQLite.
  4. Ao completar: `Verifier` → `rename` do `.part` → pós-processo.
- **Divisão dinâmica:** um worker que fica livre divide ao meio o segmento com mais bytes restantes, desde que tenha ≥ 2 MiB, alinhando o corte a 64 KiB.
- **Casos de fallback:**
  - resposta 200 na sonda: 1 conexão;
  - ETag ou tamanho diferente na retomada: zera os segmentos.

**Estados:**
```
Coletor:  Unchecked → Checking → Online | Offline | CheckFailed        Online --Iniciar--> Queued
Download: Queued → Resolving → Downloading → Verifying → Downloaded → (Extracting) → Completed
          Resolving → Waiting{until,InFlow} → Resolving (contador XFS)
          Resolving → CaptchaNeeded → Downloading | Queued (timeout ou janela fechada)
          Resolving|Downloading → Waiting{HostLimit|Quota|Backoff|Schedule} → Queued
          Downloading → Resolving (link expirou; no máximo 3 seguidas)
          Downloading → Failed{retryable}        Verifying → Failed{Integrity}
          qualquer ativo → Paused → Queued       Extracting → ExtractFailed{NeedsPassword|Corrupt|Unsafe}
```
- `next(state, ev)` é uma função pura, testada por tabela com todos os pares.
- Ao reabrir o app:
  - Resolving, Downloading, CaptchaNeeded e Verifying voltam para Queued;
  - Waiting mantém o `until`;
  - Extracting volta para Downloaded.

**Esquema SQLite:**

| Tabela | Colunas / papel |
|---|---|
| `packages` | destino, extração automática, senha do pacote, posição |
| `downloads` | url, host, estado, `wait_until`, erro, tentativas, nome, tamanho, `done_bytes`, integridade, `part`/`final_path`, etag/last_modified, conta, posição, tempos |
| `segments` | (download, idx), start, end, pos confirmado por `sync_data` |
| `archives` | arquivos compactados do pacote |
| `accounts` | `secret_ref` → cofre do SO; **nenhum segredo no banco** |
| `extract_passwords` | senha, hits |
| `settings` | chave → JSON; fonte única para app, CLI e painel |
| `history` | só inserções |
| `host_state` | esperas e cotas que sobrevivem a reinício |
| `remote_tokens` | sha256 do token, escopo panel ou extension, revogação |

## Fases (cada uma termina num portão medido)
Cada fase sai numa branch própria com um PR para `main` no `rakjsu/swoop`, e o dono revisa e faz o merge. Ao fim de cada fase eu relato o que ficou pronto, o próximo passo e as fases restantes.

**Fase 0 — Bootstrap**
- **Escopo:**
  - `git init`, README (Estado/Rodar/Layout/Desenvolvimento), CLAUDE.md (comandos e armadilhas), spec, `.gitignore`, `.editorconfig`, `.gitattributes`;
  - workspace com `core` e `api` vazios, `swoop-cli --version`, app Tauri mínimo e `ui` Vite;
  - ainda sem rede: o `rustls` com ring (e `install_default()` em todo `main`) entra na fase 1, junto com o `reqwest`.
- **CI:**
  - `rust`: ubuntu + windows, rodando fmt, clippy `-D warnings`, test e a checagem de `aws-lc-sys`;
  - `desktop`: ubuntu com libwebkit2gtk-4.1-dev, libayatana-appindicator3-dev, librsvg2-dev, libxdo-dev; roda lint e build da UI (tsc + vite) e depois o `clippy -p swoop`.
- **Portão:**
  - CI verde nos jobs `rust` (ubuntu e windows) e `desktop`;
  - `cargo tree -i aws-lc-sys` vazio (não precisa de NASM);
  - `cargo tauri dev` abre a janela no Windows do dono;
  - `git status` limpo depois de um build completo.

**Fase 1 — Motor com links diretos e CLI**
- **Escopo:** `rustls` com ring + `install_default()` em todo `main`; `core`, `net`, `store` (migração 0001), `engine`, `hosts` só com o plugin `direct`, `service` mínimo, `testsrv`, e `swoop-cli get <url> --connections 8 --limit 2M --data-dir D` e `resume`.
- **Disco:**
  - Windows: `.part` esparso (`FSCTL_SET_SPARSE`), porque o NTFS zera tudo antes de uma escrita no meio do arquivo;
  - Unix: `fs4::allocate`;
  - nos dois: checagem de espaço livre e `File::try_lock`.
- **Portão:**
  - (a) 128 MiB no CI e 1 GiB `#[ignore]`, com 8 conexões: processo morto 5 vezes em pontos aleatórios, e o sha256 bate nas 5 retomadas;
  - (b) limite de 2 MiB/s medido entre 1,8 e 2,2 MiB/s; ao trocar para 5 MiB/s em execução, a nova taxa fica em ±10% em até 2 s;
  - (c) servidor sem Range: 1 conexão, recomeça do zero e termina certo;
  - (d) ETag trocado entre sessões é detectado;
  - (e) com 1 de 8 conexões estrangulada, o tempo total fica ≤ 1,3× o normal (prova a divisão dinâmica);
  - (f) com 20 itens na fila, 3 simultâneos e 4 por servidor, os picos medidos pelo `/stats` do testsrv respeitam esses limites;
  - (g) proptest: segmentos sempre disjuntos e cobrindo `[0, tamanho)`.

**Fase 2 — UI Tauri, transporte duplo e `serve` em loopback**
- **Escopo:**
  - `api` com ts-rs;
  - `remote` só em 127.0.0.1;
  - `swoop-cli serve --ui ui/dist --port 0 --print-url`;
  - app Tauri com `exec`/`list`/`subscribe(Channel)`, `emit` só para eventos discretos, single-instance, dialog, opener, notification, window-state e bandeja;
  - telas Downloads (lista virtualizada), Adicionar links, Histórico e Opções;
  - build NSIS de fumaça no CI Windows.
- **Portão:**
  - (a) Playwright (Chromium headless) contra `serve` + testsrv: adiciona 3 links, pausar zera a velocidade em ≤ 1 s, retomar funciona e os 3 terminam com o tamanho certo. Roda no CI e aqui na nuvem;
  - (b) 45 a 55 retratos em 10 s e nenhum evento por chunk;
  - (c) no Windows, fechar com 3 downloads ativos e reabrir retoma sozinho, com sha256 ok;
  - (d) `git diff --exit-code ui/src/gen`;
  - (e) artifact NSIS gerado.

**Fase 3 — Plugins sem captcha e coletor de links**
- **Escopo:**
  - `HostPlugin`/`Registry`/`HostCtx`;
  - regras embutidas com `include_str!`, sobrescritas por `<data>/rules/hosts.toml`;
  - `detect` (texto → URLs), `check` e `expand`;
  - **Pixeldrain, Mediafire e Google Drive**;
  - coletor na UI (Verificando, Online, Offline);
  - re-resolve em 403 ou expiração; `host_state`;
  - `swoop-cli check` e `resolve --dump-fixtures`, para gerar fixtures no Windows.
- **Portão:**
  - (a) ≥ 3 fixtures por servidor, incluindo offline, captcha do Pixeldrain, malware e captcha do Mediafire, confirmação e "Too many users" do Drive;
  - (b) ≥ 10 formatos de URL por servidor classificados certo;
  - (c) testes de rede `#[ignore]` no Windows: arquivo ≥ 200 MB com hash anunciado batendo, link removido vira Offline em ≤ 5 s, pasta com contagem certa, Drive > 100 MB com matar/retomar;
  - (d) testsrv com `?expire=5s`: exatamente 1 re-resolve e o download continua do ponto;
  - (e) regra alterada no override muda o comportamento sem recompilar.

**Fase 4 — Janela de captcha, fastfile.cc (XFS) e contas premium** (dividida em 4a = captcha + XFS grátis, v0.6.0, e 4b = contas premium, v0.7.0; o desenho que valeu para a 4a está no registro de 26/09)
- **`BrowserBridge` no Tauri:**
  - `WebviewWindowBuilder` com `WebviewUrl::External`, usando o UA real do WebView2 também no reqwest;
  - `data_directory` por servidor;
  - `on_new_window` nega popups e `on_navigation` aplica a lista de bloqueio;
  - `on_download` cancela o download no WebView e entrega a URL final ao motor, com os cookies de `cookies_for_url` (chamado só em contexto async);
  - sem IPC para a página remota;
  - fila de captchas com notificação.
- **Plugin XFS genérico** (fastfile primeiro; os sites ficam nas regras):
  - grátis pela janela;
  - o contador vira `Wait` persistido;
  - premium pela API `direct_link` com chave, ou login.
- **Contas:** tela própria; `keyring` com features de plataforma explícitas (sem elas o keyring guarda em memória e perde o segredo em silêncio); `account_info`; limites por tipo de conta.
- **Portão:**
  - (a) fixtures do fastfile e de ≥ 2 outros sites XFS no CI;
  - (b) Windows, grátis: 3 downloads seguidos; o motor começa ≤ 3 s depois do clique; 0 popups; a espera sobrevive a fechar e reabrir o app (±5 s); `7z t` ok;
  - (c) Windows, premium (chave só no `.env`): `account_info` bate com o site; 5 conexões; matar/retomar ok;
  - (d) `grep` de chave ou senha em dados, logs e dump do SQLite: 0 ocorrências;
  - (e) `window.__TAURI_INTERNALS__` indefinido na janela de captcha.

**Fase 5 — Extração com 7-Zip**
- **Escopo:**
  - sidecar 7-Zip 26.03 via `externalBin`, mais `7z.dll` como resource; o script `fetch-7z` tem sha256 fixo e o `7za` não serve porque não abre RAR;
  - agrupamento de volumes (`.partN.rar`, `.rNN`, `.7z.001`, `.zNN`);
  - extração dispara quando todas as partes estão prontas;
  - ordem das senhas: do pacote, depois as mais acertadas, depois vazia;
  - `l -slt` antes de extrair, rejeitando caminho absoluto, `..` e symlink; extrai numa pasta temporária irmã e move;
  - progresso por `-bsp1`.
- **Portão:**
  - (a) fixtures pequenas no CI (ubuntu e windows): RAR5 multiparte com senha, cabeçalho cifrado, `.7z.001`, zip dividido e zip-slip;
  - (b) senha certa na 8ª posição de 10 é achada; sem a senha certa, `NeedsPassword` e 0 arquivos criados;
  - (c) zip-slip: 0 arquivos fora do destino;
  - (d) no Windows, um RAR multiparte de 4 GB extrai sozinho, com progresso monotônico.

**Fase 6 — Mega e Gofile**
- **Motor:** `RangeStyle::PathSuffix`, `ContentCodec` no worker e `Verifier` custom (o MAC do Mega é conferido num passe final).
- **Mega:** links novos e antigos; chaves, atributos, CTR e MAC (puros); hashcash em `spawn_blocking`; 509/EOVERQUOTA viram `Wait`; 404 pede novo `g`; pastas.
- **Gofile:** token de visitante ou do usuário; `X-Website-Token` com o sal vindo das regras; senha em sha256; "experimental" e desligável.
- **Regras remotas:** HTTPS, schema, cache e no máximo 1 atualização por dia.
- **Portão:**
  - (a) vetores de teste com um arquivo do dono: nome decifrado, conteúdo idêntico e MAC ok; proptest de CTR em offset arbitrário; vetor fixo de hashcash;
  - (b) rede no Windows: ≥ 1 GiB com 4 conexões e matar/retomar com MAC ok; pasta com nomes e tamanhos certos; cota vira Aguardando com o tempo do header;
  - (c) Gofile: pasta pública de 3 arquivos, pasta com senha, e sal trocado nas regras usado sem recompilar.

**Fase 7 — Captura**
- **Escopo:**
  - `clipboard-master`: debounce de 300 ms; ignora duplicatas e o que o próprio app copiou; liga/desliga pela bandeja;
  - porta da extensão em 127.0.0.1 com token de escopo `extension`;
  - extensão MV3 para Chrome, Edge e Firefox, com menus de link, seleção e página, e tela de opções com porta, token e botão de teste.
- **Portão:**
  - (a) 5 links suportados e 3 não suportados copiados: os 5 entram no coletor em ≤ 1 s e copiar de novo não duplica;
  - (b) nos 3 navegadores, o link enviado aparece em ≤ 1 s;
  - (c) testes de segurança: sem token → 401; `Origin` externa → 403; `Host` errado → 403; token de extensão em `/api/v1/exec` → 403;
  - (d) `web-ext lint` limpo.

**Fase 8 — Controle remoto na LAN**
- **Escopo:**
  - LAN desligada por padrão;
  - QR (`fast_qr`) com `http://<ip>:<porta>/#t=<token>`, token de 256 bits guardado como hash, nome do dispositivo e revogação;
  - comparação com `subtle`, 10 tentativas por minuto por IP, validação de Host e Origin, WS autenticado pela primeira mensagem, CSP;
  - layout móvel;
  - `docs/remoto.md` explicando o acesso de fora de casa pelo Tailscale.
- **Portão:**
  - (a) a 11ª tentativa recebe 429 e o token nunca aparece nos logs;
  - (b) em instalação limpa, só há sockets em 127.0.0.1;
  - (c) no celular real, o QR abre o painel em ≤ 3 s, com ≥ 4 atualizações por segundo, e pausar e adicionar funcionam;
  - (d) Playwright com viewport móvel verde.

**Instalação (pedido do dono, 25/09): Arquivos de Programas + instalador personalizado igual ao do NeoStream**
- NSIS do Tauri com `installMode: "perMachine"`: instala em `C:\Program Files\Swoop` e pede UAC.
  - Imagens de cabeçalho e barra lateral com a marca, geradas por script (como o `generate-installer-images.js` do NeoStream).
  - `installerHooks` (.nsh): fechar o `swoop.exe` antes de instalar; a desinstalação preserva os dados.
  - Idioma pt-BR.
- `apps/swoop-installer`: a "casca" do NeoStream (`installer-shell/`) portada para Tauri. Um exe único e pequeno:
  - janela sem borda, 760×500, com barra de título própria e a identidade visual do Swoop;
  - etapas Bem-vindo (Instalar / Escolher pasta), Instalando (status rotativo + barra), Concluído ("Iniciar Swoop") e Erro (Tentar novamente);
  - manifesto `requireAdministrator` (via `tauri_build::WindowsAttributes::app_manifest`);
  - o setup NSIS entra embutido com `include_bytes!` a partir de uma env var do build e roda com `/S /D=<dir>` (o `/D=` vai por último e sem aspas);
  - abre o app sem privilégio de administrador via `explorer.exe`.
  - O updater continua usando o NSIS padrão.
- O NSIS perMachine entra já no build de fumaça da Fase 2. A casca personalizada entra na Fase 9, depois que a identidade visual do app estiver definida.

**Fase 9 — Empacotamento, instalador personalizado, updater e opções restantes**
- **Escopo:**
  - NSIS `currentUser` com WebView2 `downloadBootstrapper` e as licenças do 7-Zip/unRAR;
  - updater assinado (chave nos Secrets) com `latest.json` gerado pelo tauri-action a partir das tags `v*`;
  - autostart na bandeja, agendador puro, desligar ao terminar (60 s canceláveis), AppImage e deb.
- **Portão:**
  - (a) Windows Sandbox limpo:
    - o instalador personalizado pede UAC uma vez e instala em `C:\Program Files\Swoop`, com atalhos;
    - o app abre sem privilégio de administrador (token não elevado);
    - baixa e extrai;
    - desinstalar pelo Painel de Controle preserva os dados;
  - (b) 0.1.0 atualiza para 0.1.1 e uma assinatura adulterada é recusada;
  - (c) agendador testado com relógio falso, incluindo a virada da meia-noite;
  - (d) VirusTotal registrado no release.
- **Decidido (25/09):** o repositório `Rakjsu/Swoop` é público (MIT). O updater e as regras remotas usam GitHub Releases e o raw do próprio repo.

## Testes e ambientes
- **Testes só onde é crítico:** motor, store, máquina de estados, parsers e cripto dos plugins (fixtures), extração, segurança da API e matar/retomar (`CARGO_BIN_EXE_swoop-cli`).
- **`testsrv`:**
  - conteúdo pseudoaleatório determinístico por offset, então o sha256 é conhecido sem disco;
  - botões `norange`, `etag`, `rate`, `slow`, `drop_after`, `fail`, `expire` e `gzip`;
  - `/stats` para os picos;
  - `/replay/<host>` para fluxos de vários passos sem rede.
- **Rede real:** fica em `#[ignore]`, roda no Windows do dono com `scripts/net-check.ps1`, porque o proxy da nuvem bloqueia esses sites. Chaves só no `.env` (`SWOOP_TEST_*`).
- **UI sem display:**
  - o Playwright sobe `testsrv` + `swoop-cli serve` e testa o mesmo bundle pela camada HTTP;
  - o que é só do Tauri (Channel, captcha, clipboard) fica numa checklist manual por fase.
- **Nada simulado fora dos testes:** não existe modo demo, e a UI em dev aponta para `swoop-cli serve`, com o motor real.

| | dev | test | prod |
|---|---|---|---|
| identifier | `io.github.rakjsu.swoop.dev` | — | `io.github.rakjsu.swoop` |
| dados | `app_local_data_dir` (dev) | `TempDir` | `%LOCALAPPDATA%\io.github.rakjsu.swoop\` |
| logs | debug, console + arquivo | captura do harness | info, arquivo diário, 7 dias |
| portas painel/extensão | 17391/17390 | 0 (efêmera) | 17381/17380 |
| cofre | `swoop-dev` | em memória só com `#[cfg(test)]` | `swoop` |

- **Variáveis de ambiente:** `RUST_LOG`, `SWOOP_DATA_DIR`, `SWOOP_7Z`, `SWOOP_RULES_URL`.
- **Configuração:** fica na tabela `settings`; os padrões estão em `core::Settings::default()`.

## Riscos principais → mitigação
- **Servidor muda HTML, API ou sal** → regras como dados atualizáveis, fixtures, `HostError::Changed` com mensagem clara, `net-check` semanal.
- **Cloudflare ou fingerprint TLS** → detectar o desafio e cair para `BrowserRequired` → baixar pela janela interceptando o download.
- **HTTP/2 junta os segmentos numa conexão só** → `http1_only` + `Accept-Encoding: identity`.
- **Path traversal** (nomes vindos do Content-Disposition, do Mega e de pastas) → `sanitize_component` + `safe_join`, nomes reservados do Windows e no máximo 180 caracteres.
- **Painel e extensão** → token Bearer com hash no banco, escopos por `Caller`, Host/Origin, limite de tentativas, LAN desligada, React sem `innerHTML`.
- **Segredos em log** → `Redacted<Url>` (a chave do Mega fica no `#`); nunca logar Authorization nem Cookie.
- **Senha no `-p` do 7-Zip** → processo curto, linha de comando nunca logada; investigar senha por stdin.
- **SmartScreen e antivírus** → build só pelo CI, sem UPX, aviso no README, envio de falso positivo.
- **Termos de uso** → nada de burlar esperas, cotas ou captcha; Gofile visitante marcado como experimental.

## Verificação ponta a ponta
- **Por fase:** `cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test`; na UI, `npm run build && npx playwright test`; e os testes `#[ignore]` do portão.
- **No MVP completo, no Windows do dono:**
  1. Instalar pelo NSIS.
  2. Copiar um link de cada servidor: o coletor mostra Online com nome e tamanho.
  3. Iniciar tudo: fastfile com captcha pela janela; Mega com 4 conexões; o resto em paralelo.
  4. Matar o app no meio e reabrir: tudo retoma e os hashes batem.
  5. Um RAR multiparte com senha da lista é extraído sozinho.
  6. No celular, o QR abre o painel e dá para pausar e adicionar um link.
  7. Uma extensão no Chrome envia um link.

## Registro de decisões e medições

- **25/09, fase 0:** CI verde (desktop, rust ubuntu, rust windows); `aws-lc-sys` ausente; `git status` limpo depois do build.
- **25/09, fase 1:**
  - `rusqlite_migration` trocado por migração própria via `PRAGMA user_version`, porque ele exige rustc 1.95.
  - Checkpoint da escritora a cada 500 ms ou 8 MiB. Com 1 s ou 16 MiB, sessões curtas perdiam quase tudo: 178 MiB enviados para 128 MiB, contra 144 MiB depois da mudança.
  - Portões medidos no Linux da nuvem:

| Portão | Medida |
|---|---|
| (a) 128 MiB, 8 conexões, 5 mortes | sha256 ok; progresso 12,6 → 75,8 MiB |
| (a) 1 GiB, release, 5 mortes | sha256 ok; 1043,5 MiB enviados (1,9% repetidos) |
| (b) limite | 2 MiB/s medido 2,03–2,06 em 10 s; trocado para 5 MiB/s, medido 5,02 |
| (c) sem Range | sonda + 1 requisição; sha256 ok |
| (d) ETag trocado entre sessões | detectado; arquivo novo com sha256 da nova geração |
| (e) 1 de 8 conexões a 64 KiB/s | 1,21–1,22× o tempo normal |
| (f) 20 na fila, 3 simultâneos, 2 conexões | picos: 3 downloads, 6 conexões; com 4 por servidor, pico de 4 |
| (g) proptest | invariante da tabela de segmentos mantido |
- **25/09, instalador adiantado (pedido do dono):**
  - Fase "9a" feita antes da 2: instalador personalizado igual ao do NeoStream (casca Tauri com `requireAdministrator` e setup NSIS perMachine embutido) e atualização pelo GitHub.
  - Decisão do dono: a atualização vem das Releases do GitHub, sem chave própria de assinatura (como o NeoStream). Garantias: HTTPS com certificado verificado, URL restrita a `github.com/Rakjsu/Swoop/releases/download/`, sha256 do `SHA256SUMS.txt` e nada de voltar para versão anterior.
  - Risco aceito: se a conta do GitHub for comprometida, uma release falsa seria instalada.
  - Caminho para fechar esse risco: trocar pelo `tauri-plugin-updater` com a chave privada nos Secrets. O plugin já foi conferido: usa rustls com ring, sem aws-lc.
- **25/09, fase 2a (v0.2.0), pedido do dono:** fase 2 em duas atualizações. X da janela → bandeja; notificações só de "fila terminou" e falha (v0.3.0); lista simples (pacotes na fase 3).
  - Contrato em `crates/api` com ts-rs 12 (`TS_RS_LARGE_INT = "number"`): a UI importa de `ui/src/gen`, nunca redeclara. `DownloadState` ganha TS pela feature `ts` do core.
  - O motor publica o `Snapshot` do contrato direto (sem cópia no engine) e emite `EngineEvent::Changed` a cada transição; o serviço junta rajadas num `Push::Changed`.
  - Revisão técnica conferida no código do Tauri 2.11.6: `RunEvent::Exit` é o ponto do shutdown (o `app.exit` do updater passa por ele); `Channel::send` não falha quando a página recarrega, por isso uma assinatura por vez; o opener entra só como função Rust (sem `opener:default`, que deixaria o JS abrir URLs).
  - Validado no app real (Xvfb + testsrv): adicionar, pausar, retomar, remover no meio (o `.part` some), remover concluído com e sem apagar o arquivo, limite de 2 MB/s respeitado, espera com contagem e retomada automática ao reabrir depois de matar o processo.
- **25/09, fase 2b (v0.3.0):**
  - Pedido do dono no meio da fase: pasta padrão por tipo — vídeos em `Vídeos\Swoop`, áudio em `Músicas\Swoop`, o resto em `Downloads\Swoop`. Decidido no `prepare` (depois da sonda, com o nome real); compactados ficam em "outros". Pasta escolhida à mão continua valendo para o pacote todo.
  - Preferências na tabela `settings` (migração 0002, chave `engine`, JSON); o CLI com opções explícitas não grava. Avisos `Push::Notice` (falha definitiva; "fila terminou" com contagem) viram notificação do Windows no app.
  - `crates/remote` (axum, 127.0.0.1): token de 256 bits por execução no `#t=`, comparação sem atalho, guarda de Host/Origin (DNS rebinding e sites no mesmo navegador), CSP estrita, WebSocket autenticado pela 1ª mensagem.
  - Portões medidos: (a) Playwright com o painel real — 3 links, "Pausar tudo" zera a velocidade em ≤ 1 s, retomada do ponto, 3 × 24 MiB certos no disco, histórico com 3 concluídos (25,6 s); (b) 50 retratos em 10 s pelo WebSocket e 0 outros avisos durante o download; (c) retomada ao reabrir confirmada na nuvem (processo morto); no Windows, pendente na checklist do dono; (d) `ui/src/gen` conferido no CI; (e) NSIS no job `installer`.
- **25/09, fase 3a (v0.4.0), pedido do dono:** fase 3 em duas atualizações (3a servidores, 3b coletor e pacotes recolhíveis); o dono vai liberar os domínios na rede da nuvem e pediu testes com links públicos escolhidos por mim.
  - `crates/hosts`: `HostPlugin` + `Registry` (é o `Resolver` do motor; link direto como recuo), `HostCtx` com cliente de páginas (cookies) e `Rules`. Regras em `rules/hosts.toml` embutido, mescladas chave a chave com `<dados>/rules/hosts.toml`; override ilegível cai nas embutidas com aviso.
  - Pixeldrain pela API (`/api/file/{id}/info`, sha256; `availability` ≠ vazio = captcha → `BrowserRequired`). Mediafire: API 1.5 (`get_info` com sha256, `get_content` em pedaços, subpastas até 5 níveis) + página (`#downloadButton`, `data-scrambled-url`); aviso de malware, senha e captcha viram `BrowserRequired`. Drive: `drive.usercontent.google.com/download` pedindo 1 byte; confirmação (`form#download-form` + campos ocultos) vira o link; "Too many users" → espera de cota de 60 min; login → "privado".
  - Sem rede para os sites na nuvem: fixtures reconstruídas pela estrutura pública e conferidas com projetos abertos que falam com os sites hoje — gdown (Drive: captura real de pasta, lida por `<a href>`), mediafire-dl e md_downloader (Mediafire: links que redirecionam direto para o arquivo, página de malware com `data-security-token`, senha em `for="downloadp"`), go-pixeldrain (API). Trocar por capturas reais com `swoop-cli resolve --dump-fixtures` quando a rede abrir.
  - Portões: (a) fixtures de offline, captcha do Pixeldrain, malware/senha/captcha do Mediafire, confirmação/"Too many users"/removido/privado do Drive; (b) 13–14 formatos de link por servidor + negativos (inclusive `mediafire.com.evil.example`); (d) link com validade de 0,5 s e conexão caindo a cada 3 MiB: exatamente 1 re-resolução, sha256 ok, 8 MiB + 128 KiB enviados (os 128 KiB são o que estava em trânsito nas quedas), 3/3 execuções; (e) `download_selector` trocado no override muda o resultado sem recompilar. (c) testes de rede `#[ignore]` prontos com links públicos (gdown, mediafiredl, md_downloader); pendentes até a rede abrir.
  - Portão 1e (conexão lenta) falhou no CI desta PR com 1,64×: teste de relógio dividindo a CPU com os outros testes do binário, sobre uma base de só 2 s. Movido para `crates/engine/tests/slow_connection.rs` (binário próprio) com 128 MiB e o mesmo limite de 1,3×: 1,11–1,12× sozinho e 1,12–1,13× com `download.rs` rodando em paralelo noutro processo (5/5 cada).
- **25/09, fase 3b (v0.5.0):** coletor como o do Mipony e pacotes recolhíveis.
  - Migração 0003 `collector` (URL única, `batch` por colagem, estados `unchecked/checking/online/offline/failed` em `core::LinkState`). Verificação em segundo plano, até 4 ao mesmo tempo e uma por servidor; `checking` interrompido volta para a fila ao reabrir.
  - "Iniciar" tira do coletor os escolhidos que podem baixar (offline e em verificação ficam) e cria um pacote com nome tirado dos arquivos ("album.part1.rar" + "album.part2.rar" → "album"). "Iniciar sozinho" (`Settings.auto_start`) faz isso por colagem quando ela termina de ser conferida.
  - A janela passa a mandar tudo pelo coletor (`Command::Collect`); o Playwright da fase 2 foi ajustado ao novo caminho (Coletor → "Iniciar todos online").
  - Portões medidos (Playwright, painel real): (a) 3 links válidos + 1 inexistente → 3 online com nome e tamanho e 1 offline em ≤ 5 s; "Iniciar todos online" → 1 pacote "album" com os 3, 2 MiB cada no disco; recolher esconde as 3 linhas e expandir mostra de novo; (b) "Iniciar sozinho" ligado: o link aparece em Downloads sem clique em ≤ 5 s; (c) `ui/src/gen` regenerado (CollectorView, LinkState).
- **26/09, fase 4a (v0.6.0), pedido do dono:** fase 4 em duas atualizações (4a captcha + XFS grátis, 4b contas premium). Janela = aviso + botão **Resolver**; fluxo grátis = "só o captcha" (o Swoop clica e conta o tempo, o usuário só resolve o captcha); o dono não tem conta premium (4b testada com API falsa).
  - Mudança de desenho: em vez de a janela capturar o download (`BrowserBridge`/`on_download`), o plugin XFS faz o fluxo com o reqwest e só o captcha vai para a janela. O desafio (`core::CaptchaChallenge`: tipo, chave, formulário, fim do contador) fica em `downloads.captcha`; o estado novo `captcha_needed` não ocupa vaga; a resposta volta em `ResolveRequest::captcha` e vale uma vez. Espera entre downloads (`Wait{HostLimit}`) grava `host_state` (migração 0004) e o agendador pula o servidor até lá, também depois de reabrir.
  - Janela `captcha-<id>`: página do site (a chave do captcha só vale na origem dele) com `page.js` no `initialization_script`. `window.stop()`/`document.open()` no início do documento deixam o Chromium sem pintar (medido: nenhum `requestAnimationFrame`, captura de tela trava), então os scripts do site são desligados por `MutationObserver` e a página é trocada no `DOMContentLoaded`. Sem `data_directory` próprio (perfil padrão: cookies do Google deixam o captcha mais fácil e evita ambiente WebView2 com opções diferentes no mesmo processo). Portão (e) virou "janela fora de toda capability" (ACL do Tauri nega qualquer comando; teste no app), além de `route()` (site + provedores; resposta por `swoop-captcha.invalid`) e popups/downloads negados.
  - Plugin XFS (`[xfs]` nas regras: fastfile.cc, katfile.com, ddownload.com): `download1` → contador + captcha (reCAPTCHA, hCaptcha, Turnstile, imagem, dígitos) → espera o contador → `download2` → link (302 ou página). Captcha errado/sessão perdida → pede de novo. Grátis com 1 conexão e sem retomada.
  - Portões medidos na nuvem: (a) fixtures reconstruídas do fastfile.cc (página do arquivo, reCAPTCHA + 30 s, espera, removido, só premium, página final, captcha errado, Cloudflare), katfile (imagem, 60 s) e ddownload (dígitos) + 12 formatos de link e negativos; (b) serviço inteiro contra o XFS falso: 0 envios antes do contador com a resposta pronta em t=0, resposta errada → captcha de novo (1 recusa contada pelo site), intervalo de 5 s do site → o 2º espera (≥ 4 s) antes de pedir captcha, 2 arquivos idênticos byte a byte ao servidor; motor: captcha pendente não segura a única vaga e a espera do servidor sobrevive a reabrir; (c) `route()` nega host alheio, imitações de `swoop-captcha.invalid`, http, `data:`/`javascript:`/`file:` e resposta de outro download; `Debug` do `CaptchaAnswer` esconde o token; página da janela no Chromium (Playwright): script do site não roda, HTML de dígitos num iframe `sandbox` sem scripts, widget oficial entrega o token, contador longo adia o widget, Cancelar fecha. (d) no Windows do dono (fastfile.cc real, 3 downloads seguidos, ≤ 3 s depois de resolver, 0 popups): pendente na checklist.
- **26/09, fase 4b (v0.7.0):** contas premium.
  - `core`: `Secret` (sem `Display`, `Debug` escondido), `Account`, `AccountKind` (chave da API ou login), `AccountStatus`, `AccountInfo`; `HostError::Account` (mensagem com o servidor e o motivo; falha definitiva no motor).
  - Cofre: `keyring` 3.6.3 com `windows-native`, `apple-native` e `linux-native` (sem elas cai no `mock`, conferido no código). Tabela `accounts` (migração 0005) sem segredo, uma conta por servidor; o segredo vai primeiro ao cofre, e troca/remoção apagam a entrada antiga. Nos testes da unidade, cofre em memória só com `cfg(test)`.
  - Plugins: `swoop_hosts::Accounts` no `HostCtx` (o serviço carrega do banco + cofre); `HostPlugin::{accepts_account, account_hosts, account_info}`. XFS premium pela API (`direct_link`, `account/info`) ou pelo login (`op=login` → página do arquivo com a sessão → formulário premium → link); regras `premium_connections = 5`, `premium_resumable = true`, marcadores de login e padrão da validade na página da conta. Fixtures reconstruídas: login aceito/recusado, página da conta com e sem premium, página do arquivo com a sessão premium.
  - API: `Command::{AddAccount, RemoveAccount, CheckAccount}` e `Backend::accounts` (`AccountsView` sem segredo); o painel remoto recusa comandos de conta com 403. Aba **Contas** só no app (transporte Tauri).
  - Portões medidos na nuvem: (a) serviço contra o XFS falso com chave da API: pico de 5 conexões, fechado com 8,2 MiB e reaberto (a conta volta do cofre), 24,0 MiB enviados para o arquivo de 24 MiB, arquivo idêntico byte a byte, 0 links pelo fluxo grátis; login pelo formulário dá o link em < 5 s num link com contador de 30 s no grátis; (b) o segredo aparece em 0 arquivos da pasta de dados (banco, WAL) e em 0 linhas dos logs capturados em nível debug; senha errada → "Recusada pelo site" com o motivo. App real (Xvfb, cofre do Linux): cadastro pela aba Contas → "Premium até 01/01/2099" → download sem contador. Achado: sem sessão de login o keyutils do Linux responde `ENOKEY` (o `keyring` mostra "No matching entry"); a mensagem virou "não consegui guardar a senha ou chave no cofre do sistema (…)". (c) conta real no Windows do dono: pendente (o dono ainda não tem conta).
