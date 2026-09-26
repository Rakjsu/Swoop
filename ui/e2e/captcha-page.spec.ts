import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { startStack, type Stack } from './stack';

/**
 * Página da janela de captcha (`apps/swoop/src/captcha/page.js`) no Chromium
 * (o motor do WebView2): o script entra antes da página do site, como o
 * `initialization_script` do Tauri, e a navegação para
 * `swoop-captcha.invalid` (que o Rust intercepta) é capturada aqui.
 */

const HERE = dirname(fileURLToPath(import.meta.url));
const PAGE_JS = readFileSync(join(HERE, '..', '..', 'apps', 'swoop', 'src', 'captcha', 'page.js'), 'utf8');
const ID = 7;

let stack: Stack;

test.beforeAll(async () => {
  stack = await startStack();
});

test.afterAll(async () => {
  await stack?.stop();
});

/** Mesmo que o `script()` do Rust: a configuração no lugar de SWOOP_CFG. */
async function openCaptcha(page: Page, kind: object, renderAtMs = 0): Promise<{ answer: Promise<URL> }> {
  const cfg = { id: ID, host: '127.0.0.1', kind, render_at_ms: renderAtMs };
  await page.addInitScript({ content: PAGE_JS.replace('SWOOP_CFG', JSON.stringify(cfg)) });
  const answer = new Promise<URL>((ok) => {
    void page.route('https://swoop-captcha.invalid/**', async (route) => {
      ok(new URL(route.request().url()));
      await route.fulfill({ body: 'ok' });
    });
  });
  await page.goto(`${stack.testsrv}/abcdefgh1234?size=4096`);
  // Num objeto: devolver a Promise direto faria o `await` esperar a resposta.
  return { answer };
}

test('captcha de imagem: página do site some e a resposta volta ao app', async ({ page }) => {
  const { answer } = await openCaptcha(page, { type: 'image', url: `${stack.testsrv}/captchas/test.jpg` });
  await expect(page.getByRole('heading', { name: 'Captcha de 127.0.0.1' })).toBeVisible();
  await expect(page.getByText('Free Download')).toHaveCount(0);
  // o script do site (XFS falso) não rodou
  expect(await page.evaluate(() => 'siteRan' in window)).toBe(false);
  expect(await page.title()).not.toBe('script do site');
  await expect(page.locator('img')).toHaveAttribute('src', /captchas\/test\.jpg$/);
  await page.locator('input').fill('  ab12 ');
  await page.locator('input').press('Enter');
  const url = await answer;
  expect(url.pathname).toBe('/done');
  expect(url.searchParams.get('id')).toBe(String(ID));
  expect(url.searchParams.get('token')).toBe('ab12');
});

test('captcha de dígitos: HTML do site num iframe sem scripts', async ({ page }) => {
  await openCaptcha(page, { type: 'html', html: '<b>4 7 1</b><script>parent.document.title = "pwned"</script>' });
  const frame = page.locator('iframe');
  await expect(frame).toHaveAttribute('sandbox', '');
  await expect(page.frameLocator('iframe').getByText('4 7 1')).toBeVisible();
  expect(await page.title()).not.toBe('pwned');
});

test('widget oficial (reCAPTCHA) entrega o token', async ({ page }) => {
  // O script do Google é trocado por um falso com a mesma API.
  await page.route('https://www.google.com/recaptcha/api.js*', (route) =>
    route.fulfill({
      contentType: 'text/javascript',
      body: `window.grecaptcha = { render(el, o) {
        const b = document.createElement('button');
        b.textContent = 'Não sou um robô';
        b.onclick = () => o.callback('tok+' + o.sitekey);
        el.appendChild(b);
      } };
      window.swoopOnload();`,
    }),
  );
  const { answer } = await openCaptcha(page, { type: 'recaptcha2', site_key: 'chave-do-site' });
  await page.getByRole('button', { name: 'Não sou um robô' }).click();
  expect((await answer).searchParams.get('token')).toBe('tok+chave-do-site');
});

test('contador longo: o widget só aparece perto do fim; Cancelar desiste', async ({ page }) => {
  const { answer } = await openCaptcha(
    page,
    { type: 'image', url: `${stack.testsrv}/captchas/test.jpg` },
    Date.now() + 2_500,
  );
  await expect(page.getByText(/O captcha aparece em \d s/)).toBeVisible();
  await expect(page.locator('img')).toHaveCount(0);
  await expect(page.locator('img')).toHaveCount(1, { timeout: 5_000 });
  await page.getByRole('button', { name: 'Cancelar' }).click();
  const url = await answer;
  expect(url.pathname).toBe('/cancel');
  expect(url.searchParams.get('token')).toBeNull();
});
