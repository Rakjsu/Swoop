// Janela de captcha do Swoop. Roda antes de qualquer script do site: nenhum
// deles executa, e a página é trocada por uma mínima com o widget oficial. A
// origem continua sendo a do site (a chave do captcha só vale nela). A
// resposta volta ao app navegando para https://swoop-captcha.invalid/.
// A configuração { id, host, kind, render_at_ms } entra no fim do arquivo
// (o Rust troca o marcador pelo JSON).
(function (cfg) {
  "use strict";
  if (window.top !== window || location.hostname === "swoop-captcha.invalid") return;

  // Cada <script> que o parser insere é desligado antes de rodar (o parser
  // só executa scripts conectados). A página do site fica escondida até a
  // troca. (`window.stop()` aqui deixaria o Chromium sem pintar a janela.)
  var blocker = new MutationObserver(function (records) {
    records.forEach(function (r) {
      r.addedNodes.forEach(function (n) {
        if (n.nodeName === "SCRIPT") {
          n.type = "javascript/blocked";
          n.remove();
        } else if (n.nodeName === "HTML") {
          n.style.visibility = "hidden";
        }
      });
    });
  });
  blocker.observe(document, { childList: true, subtree: true });

  var DONE = "https://swoop-captcha.invalid/";
  var WIDGETS = {
    recaptcha2: ["https://www.google.com/recaptcha/api.js", "grecaptcha"],
    hcaptcha: ["https://js.hcaptcha.com/1/api.js", "hcaptcha"],
    turnstile: ["https://challenges.cloudflare.com/turnstile/v0/api.js", "turnstile"],
  };

  // Devolve a resposta (ou a desistência) ao app.
  function send(path, token) {
    var q = "?id=" + cfg.id;
    if (token) q += "&token=" + encodeURIComponent(token);
    location.href = DONE + path + q;
  }

  // Elemento com estilo por CSSOM (funciona mesmo com CSP do site).
  function el(tag, css, text) {
    var e = document.createElement(tag);
    if (css) e.style.cssText = css;
    if (text) e.textContent = text;
    return e;
  }

  // Página mínima no lugar da do site: título, caixa do captcha, "Cancelar".
  function page() {
    var root = document.documentElement;
    while (root.firstChild) root.removeChild(root.firstChild);
    root.style.visibility = "";
    var head = el("head");
    var body = el(
      "body",
      "margin:0;padding:24px;font:15px system-ui,sans-serif;background:#111827;color:#e5e7eb"
    );
    body.id = "swoop-root";
    root.appendChild(head);
    root.appendChild(body);
    body.appendChild(el("h1", "font-size:18px;margin:0 0 6px", "Captcha de " + cfg.host));
    body.appendChild(
      el("p", "margin:0 0 18px;color:#9ca3af", "Resolva abaixo; o Swoop continua o download sozinho.")
    );
    var box = el("div", "min-height:90px");
    body.appendChild(box);
    var cancel = el("button", "margin-top:24px;padding:6px 14px", "Cancelar");
    cancel.onclick = function () {
      send("cancel");
    };
    body.appendChild(cancel);
    return { head: head, box: box };
  }

  // Widget oficial (reCAPTCHA, hCaptcha, Turnstile) renderizado na caixa.
  function widget(ui, kind) {
    var w = WIDGETS[kind.type];
    window.swoopOnload = function () {
      window[w[1]].render(ui.box, {
        sitekey: kind.site_key,
        callback: function (token) {
          send("done", token);
        },
      });
    };
    var s = el("script");
    s.src = w[0] + "?onload=swoopOnload&render=explicit";
    s.async = true;
    ui.head.appendChild(s);
  }

  // Captcha de imagem ou de dígitos: mostra e pede o texto.
  function typed(ui, kind) {
    if (kind.type === "image") {
      var img = el("img", "display:block;max-width:100%;background:#fff");
      img.src = kind.url;
      ui.box.appendChild(img);
    } else {
      // HTML do site num iframe sem scripts.
      var frame = el("iframe", "width:100%;height:90px;border:0;background:#fff");
      frame.setAttribute("sandbox", "");
      frame.srcdoc = kind.html;
      ui.box.appendChild(frame);
    }
    var input = el("input", "margin-top:12px;padding:6px;font-size:16px;width:160px");
    input.autocomplete = "off";
    var ok = el("button", "margin-left:8px;padding:6px 14px", "Enviar");
    function submit() {
      if (input.value.trim()) send("done", input.value.trim());
    }
    ok.onclick = submit;
    input.onkeydown = function (e) {
      if (e.key === "Enter") submit();
    };
    ui.box.appendChild(input);
    ui.box.appendChild(ok);
    input.focus();
  }

  // Tokens de captcha vencem em ~2 min: com contador longo, o widget só
  // aparece perto do fim para a resposta não vencer antes do envio.
  function show(ui) {
    var left = cfg.render_at_ms - Date.now();
    if (left > 0) {
      ui.box.textContent = "O captcha aparece em " + Math.ceil(left / 1000) + " s (espera do site).";
      setTimeout(function () {
        show(ui);
      }, Math.min(left, 1000));
      return;
    }
    ui.box.textContent = "";
    if (WIDGETS[cfg.kind.type]) widget(ui, cfg.kind);
    else typed(ui, cfg.kind);
  }

  // Com o HTML do site lido (e nenhum script dele rodado), troca a página.
  function start() {
    blocker.disconnect();
    show(page());
  }
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start);
  else start();
})(SWOOP_CFG);
