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
| 2 | UI de downloads, `serve` em loopback, NSIS em Arquivos de Programas | Playwright verde; retoma após reabrir |
| 3 | Pixeldrain, Mediafire, Google Drive + coletor de links | fixtures + downloads reais com hash |
| 4 | Janela de captcha, fastfile.cc (XFS), contas premium | 3 downloads grátis seguidos; segredo fora do disco |
| 5 | Extração com 7-Zip | RAR5 multiparte com senha; zip-slip bloqueado |
| 6 | Mega e Gofile | MAC do Mega ok; retomar 1 GiB |
| 7 | Área de transferência + extensão de navegador | link aparece em ≤ 1 s |
| 8 | Controle remoto na rede local (QR) | painel no celular; segurança testada |
| 9a ✅ | Instalador personalizado (estilo NeoStream) + atualização pelo GitHub (adiantado) | CI gera o instalador; release v0.1.0 publicada |
| 9 | Autostart, agendador, desligar ao terminar, AppImage/deb | instala em `C:\Program Files\Swoop`; 0.1.0 → 0.1.1 |

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
```

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
O binário sai em `target/release/swoop`. O instalador NSIS chega na fase 2.

## Layout

| Pasta | Conteúdo |
|---|---|
| `crates/core` | tipos de domínio e regras puras (estados, nomes seguros, contrato do resolvedor) |
| `crates/api` | contrato entre o motor e as interfaces (desktop, painel, extensão) |
| `crates/net` | cliente HTTP (rustls + ring, HTTP/1.1) e parsers de cabeçalhos |
| `crates/store` | SQLite numa thread própria: fila, segmentos para retomada, histórico |
| `crates/engine` | motor: agendador, segmentos com divisão dinâmica, escritora, limite de velocidade |
| `crates/hosts` | plugins de servidores (fase 1: link direto) |
| `crates/service` | liga banco + motor + plugins; trava de instância única |
| `crates/testsrv` | servidor HTTP de teste com Range e falhas simuladas (só testes) |
| `crates/update` | atualização pelas Releases do GitHub (consulta, download, sha256) |
| `apps/swoop-installer` | instalador personalizado: janela própria que roda o setup NSIS em silêncio |
| `scripts/build-release.ps1` | gera `dist/` (setup NSIS, instalador personalizado, SHA256SUMS) |
| `apps/swoop` | app desktop Tauri (janela, comandos, permissões, ícones) |
| `apps/swoop-cli` | o mesmo motor sem janela |
| `ui/` | interface React + TypeScript (a mesma para desktop e painel do celular) |
| `docs/` | desenho e decisões |

Os crates `extract` e `remote` chegam nas fases 5 e 2.

## Desenvolvimento

```bash
cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test
cd ui && npm run lint && npm run build
cargo clippy -p swoop --all-targets -- -D warnings   # app desktop (precisa de ui/dist)
```

- Código e comentários em português; identificadores em inglês.
- Cada crate tem uma responsabilidade e testa sozinho.
- Toda fase termina num portão medido.
- Segredos só no `.env` (ignorado). O app em produção não lê segredos de variáveis de ambiente.
