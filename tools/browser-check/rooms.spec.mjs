import { test, expect } from '@playwright/test';
import fs from 'node:fs/promises';
import path from 'node:path';

test.beforeEach(async ({ page }) => {
  await page.route('**/*', route => {
    const url = new URL(route.request().url());
    return url.hostname === '127.0.0.1' ? route.continue() : route.abort();
  });
  await page.goto('/tools/browser-check/harness.html');
  await expect(page.getByRole('button', { name: 'Создать комнату', exact: true })).toBeEnabled();
});

async function createRoom(page) {
  await page.getByRole('button', { name: 'Новая комната', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Вопрос или заметка' })).toBeVisible();
}
async function addMessage(page, text) {
  await page.getByRole('textbox', { name: 'Вопрос или заметка' }).fill(text);
  await page.getByRole('button', { name: 'Сохранить реплику', exact: true }).click();
  await expect(page.getByRole('log', { name: 'История комнаты' })).toContainText(text);
}
async function inspected(page) { return page.evaluate(() => window.__SOBESEDNIK_TEST__.inspect()); }

test('writing reports and build outputs does not reload a draft', async ({ page }) => {
  await createRoom(page);
  const input = page.getByRole('textbox', { name: 'Вопрос или заметка' });
  await input.fill('Черновик остаётся при записи отчёта проверки.');
  let reloads = 0; page.on('domcontentloaded', () => reloads++);
  const project = path.resolve(import.meta.dirname, '../..');
  const suffix = `watch-probe-${crypto.randomUUID()}.html`;
  const probes = [path.join(project, 'artifacts', suffix), path.join(project, 'src-tauri', 'target', suffix)];
  try {
    for (const probe of probes) await fs.writeFile(probe, '<!doctype html><p>test-only output</p>', { flag: 'wx' });
    await page.waitForTimeout(1000);
    expect(reloads).toBe(0);
    await expect(input).toHaveValue('Черновик остаётся при записи отчёта проверки.');
  } finally { for (const probe of probes) await fs.unlink(probe).catch(() => {}); }
});

test('one click creates a room without audio capture or model requests', async ({ page }) => {
  await expect(page.getByRole('button', { name: 'Начать расшифровку' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Создать комнату', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Новая комната', exact: true })).toBeVisible();
  const data = await inspected(page);
  expect(data.rooms).toHaveLength(1); expect(data.state.listening).toBe(false);
  expect(data.calls.map(c => c.command)).toEqual(['create_room']);
});

test('typed Ctrl+Enter adds history and never requests a model', async ({ page }) => {
  await createRoom(page);
  const input = page.getByRole('textbox', { name: 'Вопрос или заметка' });
  await input.fill('Обсуждаем сохранение длинной истории комнаты.'); await input.press('Control+Enter');
  await expect(page.getByRole('log')).toContainText('длинной истории');
  await expect(input).toHaveValue('');
  expect((await inspected(page)).calls.some(c => ['request_answer', 'summarize_room'].includes(c.command))).toBe(false);
});

test('summary runs only from its button and does not replace the answer', async ({ page }) => {
  await createRoom(page); await addMessage(page, 'Решили сохранять историю после каждого сообщения.');
  await page.getByRole('tab', { name: 'Итог', exact: true }).click();
  expect((await inspected(page)).calls.filter(c => c.command === 'summarize_room')).toHaveLength(0);
  await page.getByRole('button', { name: 'Подвести итог', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Обновить итог', exact: true })).toBeVisible();
  const data = await inspected(page); expect(data.state.roomSummary).toContain('сохранять историю'); expect(data.state.answer.text).toBe('');
});

test('old history is accessible across all pages without losing the earliest entry', async ({ page }) => {
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.seedLong(450));
  await expect(page.getByRole('log')).toContainText('Запись 450.');
  await expect(page.getByRole('log')).not.toContainText('АЛЬФА-731');
  await page.getByRole('button', { name: 'Ещё записи · 200' }).click();
  await page.getByRole('button', { name: 'Ещё записи · 50' }).click();
  await page.locator('[data-entry-id="5"] > details > summary').click();
  await expect(page.getByRole('log')).toContainText('АЛЬФА-731');
  expect((await inspected(page)).state.transcript).toHaveLength(450);
});

test('failed message save preserves its draft', async ({ page }) => {
  await createRoom(page); const input = page.getByRole('textbox', { name: 'Вопрос или заметка' });
  await input.fill('Эта заметка должна пережить ошибку записи.');
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.failNext('add_message'));
  await page.getByRole('button', { name: 'Сохранить реплику', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('операция не удалась');
  await expect(input).toHaveValue('Эта заметка должна пережить ошибку записи.');
  expect((await inspected(page)).rooms[0].entries).toHaveLength(0);
});

test('disabled audio sources have a visible explanation', async ({ page }) => {
  await createRoom(page);
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.clearAudioSources());
  await expect(page.getByRole('button', { name: 'Начать расшифровку', exact: true })).toBeDisabled();
  await expect(page.getByText('Выбери источник звука', { exact: true })).toBeVisible();
});

test('connection errors remain visible inside the open settings dialog', async ({ page }) => {
  await createRoom(page);
  await page.getByRole('button', { name: 'Настройки', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Настройки', exact: true });
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.failNext('refresh_models'));
  await dialog.getByRole('button', { name: 'Обновить модели', exact: true }).click();
  await expect(dialog.getByRole('alert')).toContainText('операция не удалась');
});

test('draft survives summary tab and stays isolated between rooms', async ({ page }) => {
  await createRoom(page);
  await page.getByRole('button', { name: 'Переименовать комнату', exact: true }).click();
  await page.getByRole('textbox', { name: 'Название комнаты', exact: true }).fill('Создай комнату для разговора');
  await page.getByRole('button', { name: 'Сохранить название', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Создай комнату для разговора', exact: true })).toBeVisible();
  const input = page.getByRole('textbox', { name: 'Вопрос или заметка' });
  await input.fill('Несохранённый черновик первой комнаты');
  await page.getByRole('tab', { name: 'Итог', exact: true }).click();
  await page.getByRole('tab', { name: /Разговор/ }).click();
  await expect(input).toHaveValue('Несохранённый черновик первой комнаты');
  await createRoom(page); await expect(input).toHaveValue(''); await input.fill('Черновик второй комнаты');
  await page.getByRole('button', { name: 'Открыть комнату «Создай комнату для разговора»', exact: true }).click();
  await expect(input).toHaveValue('Несохранённый черновик первой комнаты');
});

test('cancelled summary leaves no generated result behind', async ({ page }) => {
  await createRoom(page); await addMessage(page, 'Материалы для отменённого итога.');
  await page.getByRole('tab', { name: 'Итог', exact: true }).click();
  await page.getByRole('button', { name: 'Подвести итог', exact: true }).click();
  await page.getByRole('button', { name: 'Отменить подготовку итога', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Подвести итог', exact: true })).toBeEnabled();
  await page.waitForTimeout(600);
  expect((await inspected(page)).state.roomSummary).toBe('');
});

test('room tabs support keyboard navigation', async ({ page }) => {
  await createRoom(page); await page.getByRole('tab', { name: /Разговор/ }).focus();
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('tab', { name: 'Итог', exact: true })).toBeFocused();
  await expect(page.getByRole('tab', { name: 'Итог', exact: true })).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('Home');
  await expect(page.getByRole('tab', { name: /Разговор/ })).toBeFocused();
});

test('Markdown displays code and tables without executing HTML or fetching images', async ({ page }) => {
  await createRoom(page);
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.setSummary('## Рабочий итог\n\n**Важный факт**\n\n```rust\n    let value = 42;\n```\n\n| Имя | Значение |\n| --- | --- |\n| Тест | 42 |\n\n<script>window.BAD=1</script>\n<img src="https://example.invalid/pixel" onerror="window.BAD=2">\n![Фото](https://example.invalid/image)\n[опасная ссылка](javascript:alert(1))'));
  await page.getByRole('tab', { name: 'Итог', exact: true }).click();
  const panel = page.getByRole('tabpanel', { name: 'Итог', exact: true });
  await expect(panel.getByRole('heading', { name: 'Рабочий итог' })).toBeVisible();
  await expect(panel.locator('table')).toHaveCount(1);
  await expect(panel.locator('pre code')).toContainText('    let value = 42;');
  expect(await panel.locator('img, script, a[href^="javascript:"]').count()).toBe(0);
  expect(await page.evaluate(() => window.BAD)).toBeUndefined();
});

test('full history search finds an old entry and opens it in view', async ({ page }) => {
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.seedLong(450));
  await page.getByRole('searchbox', { name: /Поиск/ }).fill('АЛЬФА-731');
  await expect(page.getByRole('complementary', { name: 'Комнаты', exact: true })).toContainText('АЛЬФА-731');
  await page.getByRole('button', { name: /Открыть комнату «Длинная тестовая комната»/ }).click();
  await expect(page.getByRole('log')).toContainText('АЛЬФА-731');
  expect((await inspected(page)).state.focusEntryId).toBe('5');
  await expect(page.locator('[data-entry-id="5"] details')).toHaveAttribute('open', '');
});

test('collapsed rooms keep search and settings reachable with the keyboard', async ({ page }) => {
  await createRoom(page);
  const sidebar = page.getByRole('complementary', { name: 'Комнаты' });
  const fullWidth = (await sidebar.boundingBox()).width;
  await sidebar.getByRole('button', { name: 'Свернуть список комнат' }).press('Enter');
  expect((await sidebar.boundingBox()).width).toBeLessThan(fullWidth / 2);
  await expect(sidebar.getByRole('button', { name: 'Развернуть список комнат' })).toHaveAttribute('aria-expanded', 'false');
  await sidebar.getByRole('button', { name: 'Настройки' }).click();
  await expect(page.getByRole('dialog', { name: 'Настройки' })).toBeVisible();
  await page.keyboard.press('Escape');
  await sidebar.getByRole('button', { name: 'Открыть поиск по комнатам' }).press('Enter');
  const search = sidebar.getByRole('searchbox');
  await expect(search).toBeFocused();
  await search.fill('комната');
  await expect(sidebar.getByText('Найдено: 1')).toBeVisible();
});

test('an answer stays on the right and its history row expands independently', async ({ page }) => {
  await createRoom(page);
  await addMessage(page, 'Как разделить историю встреч?');
  await page.getByRole('button', { name: /^Получить ответ/ }).click();
  const history = page.getByRole('log');
  const row = history.locator('details');
  const answer = page.getByRole('region', { name: 'Ответ', exact: true });
  await expect(answer.getByText(/Комнаты разделяют/)).toBeVisible();
  await expect(row.locator('summary')).toBeVisible();
  await expect(row).not.toHaveAttribute('open', '');
  await row.locator('summary').press('Enter');
  await expect(row.locator('pre code')).toBeVisible();
  await row.locator('summary').press('Enter');
  await expect(row.locator('pre code')).toHaveCount(0);
  await expect(answer.locator('pre code')).toBeVisible();
});

test('live source meters and a draft work at 900px without committing the text', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 900, height: 650 });
  await createRoom(page);
  await page.getByRole('button', { name: 'Начать расшифровку' }).click();
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.audioTick(.64, 0));
  const header = page.locator('.session-header');
  await expect(header.getByRole('meter', { name: /микрофон/ })).toHaveAttribute('aria-valuenow', '-4');
  await expect(header.getByRole('meter', { name: /звук компьютера/ })).toHaveAttribute('aria-valuenow', '-60');
  await expect(header.getByRole('button', { name: 'Остановить расшифровку', exact: true })).toBeInViewport({ ratio: 1 });
  const draft = page.getByRole('textbox', { name: 'Вопрос или заметка' });
  await draft.fill('Уточнить, кто проверит макет');
  await draft.press('Control+Enter');
  await expect(page.getByRole('button', { name: 'Сохранить реплику', exact: true })).toBeDisabled();
  expect((await inspected(page)).calls.some(c => ['add_message', 'request_answer'].includes(c.command))).toBe(false);
  await page.getByRole('button', { name: 'Свернуть список комнат' }).click();
  await page.locator('.desktop-shell').screenshot({ path: testInfo.outputPath('live-draft.png') });
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.audioTick(0, .83));
  await expect(header.getByRole('meter', { name: /звук компьютера/ })).toHaveAttribute('aria-valuenow', '-2');
  await header.getByRole('button', { name: 'Остановить расшифровку', exact: true }).click();
  await expect(draft).toHaveValue('Уточнить, кто проверит макет');
  await expect(header.getByRole('meter')).toHaveCount(0);
  await page.getByRole('button', { name: 'Сохранить реплику', exact: true }).click();
  await expect(page.getByRole('log')).toContainText('Уточнить, кто проверит макет');
});

test('new speech keeps the reading position, ignores old pages and resumes following after a jump', async ({ page }) => {
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.seedLong(450));
  const log = page.getByRole('log');
  await expect(log).toContainText('Запись 450.');
  await page.getByRole('button', { name: 'Начать расшифровку' }).click();
  await log.evaluate(el => { el.scrollTop = 0; el.dispatchEvent(new Event('scroll')); });
  const old = log.locator('[data-entry-id="251"]');
  await expect(old).toBeInViewport();
  const top = (await old.boundingBox()).y;
  for (const word of ['Один', 'Два', 'Три']) {
    await page.evaluate(t => window.__SOBESEDNIK_TEST__.audioTick(.5, 0, t), word);
  }
  const jump = page.getByRole('button', { name: '3 новые реплики' });
  await expect(jump).toBeVisible();
  expect(Math.abs((await old.boundingBox()).y - top)).toBeLessThan(2);
  await page.getByRole('button', { name: 'Остановить расшифровку', exact: true }).click();
  await page.getByRole('button', { name: 'Ещё записи · 200' }).click();
  await expect(log.locator('[data-entry-id="51"]')).toHaveCount(1);
  await expect(jump).toBeVisible();
  await jump.click();
  await expect(log.getByText('Три', { exact: true })).toBeInViewport();
  await expect(jump).toHaveCount(0);
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.audioTick(.5, 0, 'Четыре'));
  await expect(log.getByText('Четыре', { exact: true })).toBeInViewport();
  await expect(page.locator('.jump-latest')).toHaveCount(0);
});

test('an older search response cannot replace results of a newer query', async ({ page }) => {
  await page.evaluate(async () => { await window.__SOBESEDNIK_TEST__.seedLong(450); window.__SOBESEDNIK_TEST__.delaySearch('АЛЬФА', 900); });
  const input = page.getByRole('searchbox', { name: /Поиск/ });
  await input.fill('АЛЬФА');
  await expect.poll(async () => (await inspected(page)).calls.some(c => c.command === 'search_rooms' && c.args.query === 'АЛЬФА')).toBe(true);
  await input.fill('НЕСУЩЕСТВУЮЩИЙ');
  await expect(page.getByText('Ничего не нашлось. Попробуй другое слово.', { exact: true })).toBeVisible();
  await page.waitForTimeout(1000);
  await expect(page.getByText('Ничего не нашлось. Попробуй другое слово.', { exact: true })).toBeVisible();
  await expect(input).toHaveValue('НЕСУЩЕСТВУЮЩИЙ');
});

test('recovery requires a separate confirmation and shows preserved file paths', async ({ page }) => {
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.seedRecovery());
  await page.getByRole('button', { name: 'Восстановить из копии', exact: true }).click();
  expect((await inspected(page)).calls.filter(c => c.command === 'recover_room')).toHaveLength(0);
  await page.getByRole('button', { name: 'Восстановить', exact: true }).click();
  await expect(page.getByRole('log')).toContainText('История из резервной копии.');
  await expect(page.getByText(/test-room.corrupt/)).toBeVisible();
  await expect(page.getByText(/test-room.unsaved/)).toBeVisible();
  expect((await inspected(page)).calls.filter(c => c.command === 'recover_room')).toHaveLength(1);
});

test('failed recovery remains explicit and can be retried without losing the recovery offer', async ({ page }) => {
  await page.evaluate(async () => { await window.__SOBESEDNIK_TEST__.seedRecovery(); window.__SOBESEDNIK_TEST__.failNext('recover_room'); });
  await page.getByRole('button', { name: 'Восстановить из копии', exact: true }).click();
  await page.getByRole('button', { name: 'Восстановить', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('операция не удалась');
  expect((await inspected(page)).state.roomRecoveries).toHaveLength(1);
  expect((await inspected(page)).rooms).toHaveLength(0);
});

for (const viewport of [{ width: 1220, height: 820 }, { width: 900, height: 650 }]) {
  test(`layout ${viewport.width} keeps actions and text within the window`, async ({ page }) => {
    await page.setViewportSize(viewport); await createRoom(page); await addMessage(page, 'Проверка расположения элементов в небольшой комнате.');
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    await expect(page.getByRole('button', { name: 'Настройки', exact: true })).toBeVisible();
    await page.getByRole('button', { name: /Получить ответ/ }).click();
    await expect(page.getByRole('region', { name: 'Ответ', exact: true })).toContainText('контекст разговоров');
    const history = await page.locator('.history-panel').boundingBox();
    const answer = await page.locator('.answer-panel').boundingBox();
    expect(history.x + history.width).toBeLessThanOrEqual(answer.x + 1);
    expect(Math.abs(history.y - answer.y)).toBeLessThanOrEqual(1);
    await expect(page.getByRole('button', { name: /^Получить ответ/ })).toBeInViewport({ ratio: 1 });
    await expect(page.getByRole('textbox', { name: 'Вопрос или заметка' })).toBeInViewport({ ratio: 1 });
    expect(await page.locator('.answer-scroll').evaluate(element => element.clientHeight)).toBeGreaterThanOrEqual(100);
    await page.screenshot({ path: `../../artifacts/rooms-test-${viewport.width}.png`, fullPage: true });
  });
}
