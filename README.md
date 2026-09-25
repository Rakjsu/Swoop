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

## Estado

| Fase | Entrega | Portão |
|---|---|---|
| 0 | Esqueleto: workspace, CLI, app Tauri mínimo, UI, CI | CI verde; janela abre no Windows ⏳ |
| 1 | Motor de download com links diretos + CLI | matar/retomar com sha256 ok; limite de velocidade ±10% |
| 2 | UI de downloads, `serve` em loopback, NSIS em Arquivos de Programas | Playwright verde; retoma após reabrir |
| 3 | Pixeldrain, Mediafire, Google Drive + coletor de links | fixtures + downloads reais com hash |
| 4 | Janela de captcha, fastfile.cc (XFS), contas premium | 3 downloads grátis seguidos; segredo fora do disco |
| 5 | Extração com 7-Zip | RAR5 multiparte com senha; zip-slip bloqueado |
| 6 | Mega e Gofile | MAC do Mega ok; retomar 1 GiB |
| 7 | Área de transferência + extensão de navegador | link aparece em ≤ 1 s |
| 8 | Controle remoto na rede local (QR) | painel no celular; segurança testada |
| 9 | Instalador personalizado, atualização automática | instala em `C:\Program Files\Swoop`; 0.1.0 → 0.1.1 |

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

# CLI (sem janela)
cargo run -p swoop-cli -- --version
```

Build de release do app:
```bash
cd apps/swoop && ../../ui/node_modules/.bin/tauri build --no-bundle
```
O binário sai em `target/release/swoop`. O instalador NSIS chega na fase 2.

## Layout

| Pasta | Conteúdo |
|---|---|
| `crates/core` | tipos de domínio e regras puras (sem I/O) |
| `crates/api` | contrato entre o motor e as interfaces (desktop, painel, extensão) |
| `apps/swoop` | app desktop Tauri (janela, comandos, permissões, ícones) |
| `apps/swoop-cli` | o mesmo motor sem janela |
| `ui/` | interface React + TypeScript (a mesma para desktop e painel do celular) |
| `docs/` | desenho e decisões |

Os crates `store`, `net`, `engine`, `hosts`, `extract`, `service`, `remote` e `testsrv` chegam nas fases seguintes.

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
