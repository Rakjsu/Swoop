import { defineConfig } from '@playwright/test';

/**
 * Testes de ponta a ponta do painel: o mesmo bundle da UI, servido pelo
 * `swoop-cli serve` com o motor real e o servidor de teste (ver e2e/stack.ts).
 * `PW_CHROMIUM` aponta para um Chromium já instalado (nuvem); no CI vem do
 * `npx playwright install chromium`.
 */
export default defineConfig({
  testDir: 'e2e',
  timeout: 120_000,
  workers: 1,
  reporter: [['list']],
  use: {
    headless: true,
    viewport: { width: 1280, height: 800 },
    launchOptions: process.env.PW_CHROMIUM ? { executablePath: process.env.PW_CHROMIUM } : {},
  },
});
