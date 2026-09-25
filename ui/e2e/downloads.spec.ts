import { expect, test } from '@playwright/test';
import { statSync } from 'node:fs';
import { join } from 'node:path';
import { startStack, type Stack } from './stack';

/**
 * Portão (a) da fase 2, pelo painel no navegador com o motor real:
 * 3 links, "Pausar tudo" zera a velocidade em ≤ 1 s, "Retomar tudo" continua
 * e os 3 terminam com o tamanho certo (na tela e no disco).
 */

const MIB = 1024 * 1024;
const SIZE = 24 * MIB;
let stack: Stack;

test.beforeAll(async () => {
  stack = await startStack();
});

test.afterAll(async () => {
  await stack?.stop();
});

test('adicionar, pausar tudo, retomar e concluir 3 downloads', async ({ page }) => {
  const downloads = join(stack.dir, 'baixados');
  await page.goto(stack.panelUrl);
  await expect(page).not.toHaveURL(/#t=/); // o token sai da barra de endereço

  // Opções: manda "outros arquivos" para a pasta do teste
  await page.getByRole('tab', { name: 'Opções' }).click();
  const others = page.getByLabel('Outros arquivos');
  await others.fill(downloads);
  await page.getByRole('button', { name: 'Salvar' }).click();
  await expect(page.getByText('Preferências salvas.')).toBeVisible();

  // Downloads: 3 links a ~2 MiB/s cada (8 conexões × 256 KiB/s)
  await page.getByRole('tab', { name: 'Downloads' }).click();
  await page.getByRole('button', { name: 'Adicionar links' }).first().click();
  const links = [1, 2, 3].map(
    (n) => `${stack.testsrv}/file/parte${n}.bin?size=${SIZE}&seed=${n}&rate=${256 * 1024}`,
  );
  await page.getByPlaceholder(/Cole um link por linha/).fill(links.join('\n'));
  await page.getByRole('button', { name: 'Adicionar 3 links' }).click();

  const rows = page.locator('.dl-row');
  await expect(rows).toHaveCount(3);
  const speed = page.locator('.statusbar-speed');
  await expect(speed).not.toHaveText('↓ 0 B/s', { timeout: 15_000 });

  // pausar zera a velocidade em até 1 s
  await page.getByRole('button', { name: 'Pausar tudo' }).click();
  await expect(speed).toHaveText('↓ 0 B/s', { timeout: 1_000 });
  await expect(rows.filter({ hasText: 'Pausado' })).toHaveCount(3);

  // retomar e terminar
  await page.getByRole('button', { name: 'Retomar tudo' }).click();
  await expect(rows.filter({ hasText: 'Concluído' })).toHaveCount(3, { timeout: 90_000 });
  await expect(rows.filter({ hasText: '24 MB' })).toHaveCount(3);
  for (const n of [1, 2, 3]) {
    expect(statSync(join(downloads, `parte${n}.bin`)).size).toBe(SIZE);
  }

  // histórico registra os três
  await page.getByRole('tab', { name: 'Histórico' }).click();
  await expect(page.locator('.hist-completed')).toHaveCount(3);
});
