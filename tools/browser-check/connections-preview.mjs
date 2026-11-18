import { chromium, expect } from '@playwright/test';
import path from 'node:path';

const project = path.resolve(import.meta.dirname, '../..');
const browser = await chromium.launch({ channel: 'chrome', headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 1220, height: 820 } });
  const external = [];
  await page.route('**/*', route => {
    const url = new URL(route.request().url());
    if (url.hostname === '127.0.0.1') return route.continue();
    external.push(url.origin); return route.abort();
  });
  await page.goto('http://127.0.0.1:1421/tools/browser-check/harness.html');
  await expect(page.getByRole('button', { name: 'Создать комнату', exact: true })).toBeEnabled();
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.readyProviders());
  await page.getByRole('button', { name: 'Настройки', exact: true }).click();
  await page.getByRole('button', { name: 'Подключения', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Подключения', exact: true });
  await expect(dialog).toBeVisible();
  for (const width of [1220, 900]) {
    await page.setViewportSize({ width, height: width === 900 ? 650 : 820 });
    await dialog.evaluate(element => { element.scrollTop = 0; });
    expect(await dialog.evaluate(element => element.scrollWidth <= element.clientWidth + 1)).toBe(true);
    await page.screenshot({ path: path.join(project, `artifacts/connections-preview-${width}.png`), fullPage: true });
    for (const id of ['chatgpt', 'antigravity', 'claude']) {
      const picker = dialog.locator(`#connection-model-${id}`);
      await picker.scrollIntoViewIfNeeded();
      await expect(picker).toBeVisible();
    }
    if (width === 900) await page.screenshot({ path: path.join(project, 'artifacts/connections-preview-900-claude.png'), fullPage: true });
  }
  expect(external).toEqual([]);
  console.log('Saved connections previews with synthetic data at 1220 and 900 pixels; no external requests.');
} finally { await browser.close(); }
