import { expect, test } from '@playwright/test';
import { startStack, type Stack } from './stack';

/**
 * Fase 4a no painel do navegador: um link de XFileSharing (o XFS falso do
 * testsrv) passa pelo coletor e fica em "Captcha pendente". O captcha só se
 * resolve na janela do app, então o painel avisa em vez de mostrar o botão.
 */

let stack: Stack;

test.beforeAll(async () => {
  stack = await startStack({ xfs: true });
});

test.afterAll(async () => {
  await stack?.stop();
});

test('captcha pendente aparece e o painel manda resolver no app', async ({ page }) => {
  await page.goto(stack.panelUrl);
  await page.getByRole('button', { name: 'Adicionar links' }).first().click();
  await page.getByPlaceholder(/Cole os links/).fill(`${stack.testsrv}/abcdefgh1234?countdown=0&size=65536`);
  await page.getByRole('button', { name: 'Adicionar', exact: true }).click();
  const collected = page.locator('.col-row');
  await expect(collected.filter({ hasText: 'Online' })).toHaveCount(1, { timeout: 5_000 });
  await expect(collected).toContainText('abcdefgh1234.bin');

  await page.getByRole('button', { name: 'Iniciar todos online (1)' }).click();
  const row = page.locator('.dl-row', { hasText: 'abcdefgh1234' });
  await expect(row).toContainText('Captcha pendente', { timeout: 10_000 });
  await expect(row).toContainText('resolva no app do computador');
  await expect(row.getByRole('button', { name: 'Resolver captcha' })).toHaveCount(0);

  // Pausar vale enquanto espera o captcha.
  await row.getByRole('button', { name: 'Pausar' }).click();
  await expect(row).toContainText('Pausado');
});
