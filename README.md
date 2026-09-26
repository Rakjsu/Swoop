# Swoop

Gerenciador de downloads desktop no estilo do Mipony, em Rust + Tauri 2.

Você cola (ou copia) links de servidores de hospedagem e o Swoop faz o resto:
- descobre o link real, com espera e captcha resolvidos por você numa janela do próprio app;
- baixa em fila, com várias conexões por arquivo;
- retoma de onde parou mesmo depois de fechar o app;
- descompacta RAR/ZIP/7z usando a sua lista de senhas;
- pode ser acompanhado e controlado pelo celular.

Servidores previstos: fastfile.cc (e outros XFileSharing), Mega, Mediafire, Google Drive, Gofile e Pixeldrain.

O Swoop respeita as regras dos servidores:
- não burla esperas, cotas nem captchas;
- a contagem regressiva aparece na tela;
- contas premium são as do próprio usuário, guardadas no cofre do sistema.

## Instalar (Windows)

1. Baixe o **`Swoop-Installer-<versão>.exe`** em [Releases](https://github.com/Rakjsu/Swoop/releases/latest).
2. O instalador ainda não tem assinatura de código, então o Windows pode mostrar "O Windows protegeu o computador". Clique em **Mais informações → Executar assim mesmo**.
3. Aceite o pedido de administrador e clique em **Instalar**. O Swoop vai para `C:\Program Files\Swoop`, com atalhos no Menu Iniciar e na área de trabalho.

**Atualizações:** ao abrir, e a cada 6 h, o app procura versão nova nas Releases. Se houver, aparece a faixa "Versão X disponível — **Atualizar**". O clique baixa o instalador, confere o sha256 publicado, pede o administrador e reabre o Swoop atualizado.

**Alternativa:** `Swoop_<versão>_x64-setup.exe` é o instalador NSIS padrão. Ele também instala o WebView2 se estiver faltando.

## Estado

| Fase | Entrega | Portão |
|---|---|---|
| 0 ✅ | Esqueleto: workspace, CLI, app Tauri mínimo, UI, CI | CI verde (ubuntu + windows); janela abre no Windows ⏳ |
| 1 ✅ | Motor de download com links diretos + CLI | 5 mortes + retomada, sha256 ok (128 MiB e 1 GiB); limite 2,03/5,02 MiB/s; conexão lenta 1,22× |
| 2a ✅ | Tela de downloads: adicionar links, pausar/retomar/remover, limite, bandeja (v0.2.0) | testado no app real com o servidor de teste; retoma após reabrir |
| 2b ✅ | Histórico, opções salvas, pasta automática por tipo, notificações, painel no navegador (`serve`) (v0.3.0) | Playwright: pausar tudo zera em ≤ 1 s e 3 × 24 MB certos; 50 retratos em 10 s |
| 3a ✅ | Pixeldrain, Mediafire e Google Drive (pastas viram arquivos), regras editáveis, `swoop-cli check`/`resolve` (v0.4.0) | fixtures de cada caso; link expirado → 1 re-resolução e segue do ponto; testes de rede prontos ⏳ |
| 3b ✅ | Coletor de links como o do Mipony (confere antes de baixar, "iniciar sozinho") e pacotes recolhíveis (v0.5.0) | Playwright: 3 online + 1 offline em ≤ 5 s, pacote com os 3 certos no disco, recolher esconde |
| 4a ✅ | Janela de captcha (botão Resolver), fastfile.cc e outros XFileSharing grátis, esperas entre downloads por servidor (v0.6.0) | XFS falso: 0 envios antes do contador, captcha errado pede de novo, intervalo do site segura o servidor, arquivos idênticos; página do captcha no Chromium sem scripts do site; 3 downloads reais no Windows ⏳ |
| 4b | Contas premium no cofre do Windows, aba Contas | 5 conexões com retomada; segredo fora do disco |
| 5 | Extração com 7-Zip | RAR5 multiparte com senha; zip-slip bloqueado |
| 6 | Mega e Gofile | MAC do Mega ok; retomar 1 GiB |
| 7 | Área de transferência + extensão de navegador | link aparece em ≤ 1 s |
| 8 | Controle remoto na rede local (QR) | painel no celular; segurança testada |
| 9a ✅ | Instalador personalizado (estilo NeoStream) + atualização pelo GitHub (adiantado) | release v0.1.0 instalada; 0.1.0 → 0.1.1 pela faixa ✅ |
| 9 | Autostart, agendador, desligar ao terminar, AppImage/deb | Windows Sandbox limpo |

Plano completo: [`docs/specs/2026-09-25-swoop-design.md`](docs/specs/2026-09-25-swoop-design.md).

## Rodar

Requisitos:
- Rust estável (≥ 1.93);
- Node 22;
- no Windows, o WebView2 (já vem no Windows 10/11);
- no Linux, `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev`.

```bash
# dependências da interface (uma vez)
cd ui && npm install && cd ..

# app desktop em modo dev (identificador separado do instalado)
cd apps/swoop && ../../ui/node_modules/.bin/tauri dev --config tauri.dev.conf.json

# CLI (sem janela): baixa com 8 conexões e limite de 2 MiB/s
cargo run --release -p swoop-cli -- get "https://exemplo.com/arquivo.zip" --connections 8 --limit 2M
cargo run --release -p swoop-cli -- resume      # continua o que ficou pela metade
cargo run --release -p swoop-cli -- list        # mostra a fila

# painel no navegador (127.0.0.1): imprime o endereço com o token
cargo run --release -p swoop-cli -- serve --ui ui/dist

# confere links sem baixar (aceita texto com links no meio; pastas são abertas)
cargo run --release -p swoop-cli -- check "https://pixeldrain.com/u/abc123 https://drive.google.com/file/d/…/view"
# mostra o link direto e grava as respostas lidas como fixtures (conferir antes de commitar)
cargo run --release -p swoop-cli -- resolve "https://www.mediafire.com/file/…/file" --dump-fixtures fx/
```

**Servidores (fase 3a):** Pixeldrain, Mediafire e Google Drive, além de links diretos. Links de pasta (Drive, Mediafire) e de lista (Pixeldrain) viram um download por arquivo. Captcha, arquivo marcado como perigoso ou com senha, e "muitos downloads" do Drive não são contornados: o download mostra o motivo (ou espera, no caso da cota).

**Captcha e XFileSharing (fase 4a):** links de fastfile.cc, katfile.com e ddownload.com (outros sites XFileSharing entram pelas regras) seguem o caminho grátis do site: o Swoop clica os botões e conta o tempo sozinho; quando o site pede captcha, o download fica em **Captcha pendente**, chega um aviso do Windows e o botão **Resolver captcha** abre uma janela só com o captcha do site. Ao resolver, a janela fecha e o download continua depois do contador (nunca antes). "Espere N minutos até o próximo download" vale para o servidor inteiro, inclusive depois de fechar e reabrir o app. Pelo painel no navegador o captcha não se resolve: ele avisa para usar o app do computador.

**Coletor:** "Adicionar links" manda o texto colado para a aba **Coletor**, que acha os links (até no meio de frases), abre as pastas e confere cada um no servidor (online, nome, tamanho). "Iniciar" leva os escolhidos para a fila num pacote com o nome dos arquivos; com **Iniciar sozinho**, cada colagem vai para a fila assim que termina de ser conferida. Na aba Downloads, cada pacote tem um cabeçalho que recolhe e expande.

**Regras dos servidores:** seletores, endereços e textos que o Swoop procura nas páginas ficam em `rules/hosts.toml` (embutido). Para corrigir uma mudança de site sem esperar versão nova, crie `<pasta de dados>/rules/hosts.toml` só com o que muda, por exemplo:
```toml
[mediafire]
download_selector = "a#novoBotao"
```

**Pastas automáticas:** sem pasta escolhida ao adicionar, vídeos vão para `Vídeos\Swoop`, áudio para `Músicas\Swoop` e o resto para `Downloads\Swoop` (trocáveis em Opções).

Ctrl+C grava o progresso antes de sair; matar o processo perde no máximo o último meio segundo.
Pasta de dados: `SWOOP_DATA_DIR` ou `%LOCALAPPDATA%\io.github.rakjsu.swoop` (Windows).
Destino padrão: `Downloads/Swoop`.

Servidor de teste local (arquivo de tamanho e sha256 conhecidos, para ensaios):
```bash
cargo run --release -p swoop-testsrv -- 8765
```

Build de release do app:
```bash
cd apps/swoop && ../../ui/node_modules/.bin/tauri build --no-bundle
```
O binário sai em `target/release/swoop`. Instaladores: `scripts/build-release.ps1` (Windows).

No app, o X da janela só esconde: os downloads continuam e o ícone da bandeja reabre, pausa/retoma tudo ou sai (gravando o progresso).

## Layout

| Pasta | Conteúdo |
|---|---|
| `crates/core` | tipos de domínio e regras puras (estados, nomes seguros, contrato do resolvedor) |
| `crates/api` | contrato entre o motor e as interfaces: `Command`, `DownloadView`, `Snapshot`, trait `Backend`; gera os tipos TypeScript de `ui/src/gen` |
| `crates/net` | cliente HTTP (rustls + ring, HTTP/1.1) e parsers de cabeçalhos |
| `crates/store` | SQLite numa thread própria: fila, segmentos para retomada, histórico |
| `crates/engine` | motor: agendador, segmentos com divisão dinâmica, escritora, limite de velocidade |
| `crates/hosts` | plugins de servidores: link direto, Pixeldrain, Mediafire, Google Drive, XFileSharing (fastfile.cc…); regras em `rules/hosts.toml`; `detect` (links num texto) |
| `crates/service` | liga banco + motor + plugins; trava de instância única; implementa `Backend` |
| `crates/remote` | painel no navegador: arquivos da UI + API HTTP/WebSocket em 127.0.0.1, com token |
| `crates/testsrv` | servidor HTTP de teste com Range e falhas simuladas (só testes) |
| `crates/update` | atualização pelas Releases do GitHub (consulta, download, sha256) |
| `apps/swoop-installer` | instalador personalizado: janela própria que roda o setup NSIS em silêncio |
| `scripts/build-release.ps1` | gera `dist/` (setup NSIS, instalador personalizado, SHA256SUMS) |
| `apps/swoop` | app desktop Tauri: comandos (`exec`, `list`, `subscribe` via Channel), bandeja, encerramento, janela de captcha (`src/captcha/`) |
| `apps/swoop-cli` | o mesmo motor sem janela |
| `ui/` | interface React + TypeScript: `transport/` (IPC do Tauri; HTTP na v0.3.0), `downloads/` (tela), `gen/` (tipos gerados, não editar) |
| `rules/hosts.toml` | regras dos servidores (dados, sobrescrevíveis pelo usuário) |
| `docs/` | desenho e decisões |

O crate `extract` chega na fase 5.

## Desenvolvimento

```bash
cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test
cd ui && npm run lint && npm run build
cargo clippy -p swoop --all-targets -- -D warnings   # app desktop (precisa de ui/dist)
cargo build -p swoop-cli -p swoop-testsrv && cd ui && npm run e2e   # Playwright (painel real)
```

- Código e comentários em português; identificadores em inglês.
- Cada crate tem uma responsabilidade e testa sozinho.
- Toda fase termina num portão medido.
- Segredos só no `.env` (ignorado). O app em produção não lê segredos de variáveis de ambiente.
