# Fixtures do Rapidgator (reconstruídas)

A nuvem de desenvolvimento não alcança o rapidgator.net. Estas páginas foram
**reconstruídas** a partir de fontes abertas recentes, não capturadas:

- JDownloader `RapidGatorNet.java` e `RapidGatorNetFolder.java` (espelho de 09/2026);
- pyload `downloaders/RapidgatorNet.py` 0.60 (Turnstile desde 12/2025) e as
  issues #3508, #4358, #4640 e #4672 (respostas reais do contador);
- oc-proxy-downloader `docs/HOSTER_CASES.md` (10/2026): formulário
  `#captchaform`, limite de 500 MB e "1 arquivo a cada 120 min";
- clientes da API v2 (gopeed-ext-debrid 10/2026, rapidgator-api 1.1.0).

Ids, `sid`, `session_id`, tokens, e-mails e chaves de captcha são falsos.
Com o site ao alcance, trocar pelas capturas reais de
`swoop-cli resolve <link> --dump-fixtures <pasta>` (atenção: `resolve` começa
um download grátis de verdade).
