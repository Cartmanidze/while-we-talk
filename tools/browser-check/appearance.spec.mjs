import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.route('**/*', route => new URL(route.request().url()).hostname === '127.0.0.1' ? route.continue() : route.abort());
  await page.goto('/tools/browser-check/harness.html');
  await page.getByRole('button', { name: 'Создать комнату', exact: true }).click();
});

async function windowPanel(page) {
  await page.getByRole('button', { name: 'Настройки', exact: true }).click();
  return page.getByRole('region', { name: 'Окно', exact: true });
}

test('opacity changes and reset work while listening without requesting a model', async ({ page }) => {
  await page.getByRole('button', { name: 'Начать расшифровку', exact: true }).click();
  const panel = await windowPanel(page);
  const slider = panel.getByRole('slider', { name: 'Непрозрачность окна' });
  await expect(slider).toBeEnabled();
  await slider.focus(); await slider.press('Home');
  await expect.poll(() => page.evaluate(() => window.__SOBESEDNIK_TEST__.inspect().state.settings.windowOpacity)).toBe(30);
  await panel.getByRole('button', { name: 'Сделать окно непрозрачным', exact: true }).click();
  await expect.poll(() => page.evaluate(() => window.__SOBESEDNIK_TEST__.inspect().state.settings.windowOpacity)).toBe(100);
  const result = await page.evaluate(() => window.__SOBESEDNIK_TEST__.inspect());
  expect(result.state.listening).toBe(true);
  expect(result.calls.filter(call => call.command === 'update_window_appearance').map(call => call.args.windowOpacity)).toEqual([30, 100]);
  expect(result.calls.some(call => ['request_answer', 'summarize_room'].includes(call.command))).toBe(false);
});

test('capture checkbox updates only window appearance', async ({ page }) => {
  const panel = await windowPanel(page);
  const checkbox = panel.getByRole('checkbox', { name: 'Скрывать при демонстрации экрана' });
  await expect(checkbox).toBeChecked();
  await checkbox.uncheck();
  await expect.poll(() => page.evaluate(() => window.__SOBESEDNIK_TEST__.inspect().state.settings.hideFromCapture)).toBe(false);
  await checkbox.check();
  await expect.poll(() => page.evaluate(() => window.__SOBESEDNIK_TEST__.inspect().state.settings.hideFromCapture)).toBe(true);
  const commands = await page.evaluate(() => window.__SOBESEDNIK_TEST__.inspect().calls.map(call => call.command));
  expect(commands).toEqual(['create_room', 'update_window_appearance', 'update_window_appearance']);
});

test('failed opacity application restores the confirmed value and shows its error', async ({ page }) => {
  const panel = await windowPanel(page);
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.failNext('update_window_appearance'));
  const slider = panel.getByRole('slider', { name: 'Непрозрачность окна' });
  await slider.focus(); await slider.press('Home');
  await expect(slider).toHaveValue('100');
  await expect(page.getByRole('dialog', { name: 'Настройки' }).getByRole('alert')).toContainText('операция не удалась');
  expect(await page.evaluate(() => window.__SOBESEDNIK_TEST__.inspect().state.settings.windowOpacity)).toBe(100);
});
