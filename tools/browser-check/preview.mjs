import { chromium, expect } from '@playwright/test';
import path from 'node:path';

const project = path.resolve(import.meta.dirname, '../..');
const browser = await chromium.launch({ channel: 'chrome', headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 1220, height: 820 } });
  await page.route('**/*', route => new URL(route.request().url()).hostname === '127.0.0.1' ? route.continue() : route.abort());
  await page.goto('http://127.0.0.1:1421/tools/browser-check/harness.html');
  await expect(page.getByRole('button', { name: 'Создать комнату', exact: true })).toBeEnabled();
  await page.screenshot({ path: path.join(project, 'artifacts/minimal-welcome.png'), fullPage: true });
  await page.getByRole('button', { name: 'Создать комнату', exact: true }).click();
  await page.getByRole('textbox', { name: 'Вопрос или заметка' }).fill('Обсуждаем архитектуру локального помощника. Комнаты разделяют историю. Итоги составляются по запросу. Следующий шаг — проверить восстановление.');
  await page.getByRole('button', { name: 'Сохранить реплику', exact: true }).click();
  await page.getByRole('button', { name: 'Переименовать комнату', exact: true }).click();
  await page.getByRole('textbox', { name: 'Название комнаты', exact: true }).fill('Архитектура локального помощника');
  await page.getByRole('button', { name: 'Сохранить название', exact: true }).click();
  await page.getByRole('button', { name: /Закрепить комнату/ }).click();
  await page.getByRole('button', { name: /Получить ответ/ }).click();
  await expect(page.getByRole('region', { name: 'Ответ', exact: true })).toContainText('контекст разговоров');
  await page.getByRole('tab', { name: 'Итог', exact: true }).click();
  await page.getByRole('button', { name: 'Подвести итог', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Обновить итог', exact: true })).toBeVisible();
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.previewWithoutTimings());
  await page.screenshot({ path: path.join(project, 'artifacts/minimal-ui-1220.png'), fullPage: true });
  await page.setViewportSize({ width: 900, height: 650 });
  await page.screenshot({ path: path.join(project, 'artifacts/minimal-ui-900.png'), fullPage: true });
  console.log('Saved isolated UI previews with synthetic data.');
} finally { await browser.close(); }
