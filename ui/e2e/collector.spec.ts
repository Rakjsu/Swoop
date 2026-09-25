import { expect, test } from '@playwright/test';
import { statSync } from 'node:fs';
import { join } from 'node:path';
import { startStack, type Stack } from './stack';

/**
 * Portões da fase 3b, pelo painel com o motor real:
 * (a) 3 links válidos + 1 inexistente → 3 online com nome e tamanho e 1
 *     offline em ≤ 5 s; "Iniciar todos online" → um pacote com os 3, que
 *     terminam com o tamanho certo; recolher o pacote esconde as 3 linhas;
 * (b) com "Iniciar sozinho", o link vai para a fila sem clique.
 */

const MIB = 1024 * 1024;
let stack: Stack;

test.beforeAll(async () => {
  stack = await startStack();
});

test.afterAll(async () => {
  await stack?.stop();
});

test('coletor confere, inicia num pacote e o pacote recolhe', async ({ page }) => {
  const downloads = join(stack.dir, 'baixados');
  await page.goto(stack.panelUrl);
  await page.getByRole('tab', { name: 'Opções' }).click();
  await page.getByLabel('Outros arquivos').fill(downloads);
  await page.getByRole('button', { name: 'Salvar' }).click();
  await expect(page.getByText('Preferências salvas.')).toBeVisible();

  await page.getByRole('tab', { name: 'Coletor' }).click();
  await page.getByRole('button', { name: 'Adicionar links' }).first().click();
  const links = [1, 2, 3].map((n) => `${stack.testsrv}/file/album.part${n}.rar?size=${2 * MIB}&seed=${n}`);
  const gone = `${stack.testsrv}/sumiu/nada.bin`;
  await page.getByPlaceholder(/Cole os links/).fill(`Links do álbum: ${links.join(' ')} e ${gone}`);
  const t0 = Date.now();
  await page.getByRole('button', { name: 'Adicionar 4 links' }).click();

  const rows = page.locator('.col-row');
  await expect(rows).toHaveCount(4);
  await expect(rows.filter({ hasText: 'Online' })).toHaveCount(3, { timeout: 5_000 });
  await expect(rows.filter({ hasText: 'Offline' })).toHaveCount(1, { timeout: 5_000 });
  expect(Date.now() - t0).toBeLessThanOrEqual(5_000);
  await expect(rows.filter({ hasText: 'album.part2.rar' })).toContainText('2 MB');

  await page.getByRole('button', { name: 'Iniciar todos online (3)' }).click();
  await expect(page.getByRole('tab', { name: 'Downloads' })).toHaveAttribute('aria-selected', 'true');
  const pkg = page.locator('.pkg-row', { hasText: 'album' });
  await expect(pkg).toHaveCount(1);
  const files = page.locator('.dl-row', { hasText: 'album.part' });
  await expect(files.filter({ hasText: 'Concluído' })).toHaveCount(3, { timeout: 30_000 });
  for (const n of [1, 2, 3]) {
    expect(statSync(join(downloads, `album.part${n}.rar`)).size).toBe(2 * MIB);
  }
  await expect(pkg).toContainText('Concluído');

  // recolher esconde as linhas do pacote; expandir mostra de novo
  await page.getByRole('button', { name: 'Recolher album' }).click();
  await expect(files).toHaveCount(0);
  await page.getByRole('button', { name: 'Expandir album' }).click();
  await expect(files).toHaveCount(3);

  // o offline ficou no coletor até ser removido
  await page.getByRole('tab', { name: 'Coletor' }).click();
  await expect(rows).toHaveCount(1);
  await page.getByRole('button', { name: 'Remover offline' }).click();
  await expect(rows).toHaveCount(0);
});

test('iniciar sozinho manda para a fila sem clique', async ({ page }) => {
  await page.goto(stack.panelUrl);
  await page.getByRole('tab', { name: 'Coletor' }).click();
  const auto = page.getByLabel('Iniciar sozinho');
  await auto.check();
  await expect(auto).toBeChecked();

  await page.getByRole('button', { name: 'Adicionar links' }).first().click();
  await page.getByPlaceholder(/Cole os links/).fill(`${stack.testsrv}/file/sozinho.bin?size=${MIB}&seed=9`);
  await page.getByRole('button', { name: 'Adicionar' }).last().click();

  await page.getByRole('tab', { name: 'Downloads' }).click();
  await expect(page.locator('.dl-row', { hasText: 'sozinho.bin' })).toHaveCount(1, { timeout: 5_000 });
});
