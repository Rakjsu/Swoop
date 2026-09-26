# Fixtures do XFileSharing

Páginas do fluxo grátis de sites XFileSharing Pro (fastfile.cc com reCAPTCHA,
katfile.com com captcha de imagem, ddownload.com com dígitos posicionados):
página do arquivo (`op=download1`), página do download grátis (contador,
captcha, formulário `F1`/`op=download2`), espera entre downloads, arquivo
removido, só premium, Cloudflare, captcha errado e página final com o link.
Conta premium (`fastfile_login_*`, `fastfile_my_account*`,
`fastfile_premium_page`): login aceito/recusado, página da conta com e sem
premium e a página do arquivo vista com a sessão premium.
Reconstruídas a partir da estrutura pública do XFileSharing Pro (a rede da
nuvem bloqueia os sites), com ids e chaves falsos. Trocar por capturas reais
com `swoop-cli resolve <link> --dump-fixtures <pasta>`.
