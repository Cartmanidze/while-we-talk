import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.route('**/*', route => new URL(route.request().url()).hostname === '127.0.0.1' ? route.continue() : route.abort());
});
async function load(page, query = '') { await page.goto(`/tools/browser-check/harness.html${query}`); }
async function createRoom(page) {
  await page.getByRole('button', { name: 'Создать комнату', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Вопрос или заметка' })).toBeVisible();
}
const controls = page => page.getByRole('dialog', { name: 'Настройки', exact: true });
const inspect = page => page.evaluate(() => window.__SOBESEDNIK_TEST__.inspect());

test('a menu request pending before the initial state opens settings without other actions', async ({ page }) => {
  await load(page, '?menuRequestId=7');
  await expect(controls(page)).toBeVisible();
  const data = await inspect(page);
  expect(data.state.menuRequestId).toBe(7);
  expect(data.rooms).toHaveLength(0);
  expect(data.state.listening).toBe(false);
  expect(data.calls).toEqual([]);
});

test('each menu request opens once, while a later request reopens the closed menu', async ({ page }) => {
  await load(page); await createRoom(page);
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.hotkeyMenu());
  await expect(controls(page)).toBeVisible();
  await controls(page).getByRole('button', { name: 'Закрыть настройки', exact: true }).click();
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.setSummary('Повторный app-state с тем же запросом меню.'));
  await expect(controls(page)).not.toBeVisible();
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.hotkeyMenu());
  await expect(controls(page)).toBeVisible();
  const data = await inspect(page);
  expect(data.calls.map(call => call.command)).toEqual(['create_room']);
});

test('a global menu request replaces the advanced settings dialog', async ({ page }) => {
  await load(page); await createRoom(page);
  await page.getByRole('button', { name: 'Настройки', exact: true }).click();
  await controls(page).getByRole('button', { name: 'Пути к программам', exact: true }).click();
  const advanced = page.getByRole('dialog', { name: 'Пути к программам', exact: true });
  await expect(advanced).toBeVisible();
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.hotkeyMenu());
  await expect(advanced).not.toBeVisible();
  await expect(controls(page)).toBeVisible();
});

test('a newer menu request survives the previous dialog close event', async ({ page }) => {
  await load(page); await createRoom(page);
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.hotkeyMenu());
  await expect(controls(page)).toBeVisible();
  await page.evaluate(async () => {
    document.querySelector('.controls-dialog').close();
    await window.__SOBESEDNIK_TEST__.hotkeyMenu();
  });
  await expect(controls(page)).toBeVisible();
});

test('a new room shortcut closes menus, returns to conversation, and preserves the old draft', async ({ page }) => {
  await load(page); await createRoom(page);
  await page.getByRole('button', { name: 'Переименовать комнату', exact: true }).click();
  await page.getByRole('textbox', { name: 'Название комнаты', exact: true }).fill('Исходная комната');
  await page.getByRole('button', { name: 'Сохранить название', exact: true }).click();
  const draft = page.getByRole('textbox', { name: 'Вопрос или заметка' });
  await draft.fill('Несохранённый текст исходной комнаты');
  await page.getByRole('tab', { name: 'Итог', exact: true }).click();
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.hotkeyMenu());
  await expect(controls(page)).toBeVisible();
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.hotkeyNewRoom());
  await expect(controls(page)).not.toBeVisible();
  await expect(page.getByRole('tab', { name: 'Разговор', exact: true })).toHaveAttribute('aria-selected', 'true');
  await expect(draft).toHaveValue('');
  const data = await inspect(page);
  expect(data.rooms).toHaveLength(2); expect(data.state.menuRequestId).toBe(0);
  expect(data.state.listening).toBe(false); expect(data.state.busy).toBe(false);
  await page.getByRole('button', { name: 'Открыть комнату «Исходная комната»', exact: true }).click();
  await expect(draft).toHaveValue('Несохранённый текст исходной комнаты');
  expect((await inspect(page)).calls.some(call => ['start_listening', 'request_answer', 'summarize_room'].includes(call.command))).toBe(false);
});

test('a new room shortcut clears a search target without replaying a previous menu request', async ({ page }) => {
  await load(page); await page.evaluate(() => window.__SOBESEDNIK_TEST__.seedLong(450));
  await page.getByRole('searchbox', { name: /Поиск/ }).fill('АЛЬФА-731');
  await page.getByRole('button', { name: 'Открыть комнату «Длинная тестовая комната»', exact: true }).click();
  await expect(page.locator('.search-match[data-entry-id="5"]')).toBeVisible();
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.hotkeyMenu());
  await expect(controls(page)).toBeVisible();
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.hotkeyNewRoom());
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.setSummary('Обычное обновление новой комнаты.'));
  await expect(controls(page)).not.toBeVisible();
  await expect(page.locator('.search-match')).toHaveCount(0);
  await expect(page.getByRole('textbox', { name: 'Вопрос или заметка' })).toBeVisible();
});

test('a rejected new-room shortcut keeps the listening room and shows its error', async ({ page }) => {
  await load(page); await createRoom(page);
  const before = await inspect(page);
  await page.getByRole('button', { name: 'Начать расшифровку', exact: true }).click();
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.hotkeyMenu());
  await expect(controls(page)).toBeVisible();
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.hotkeyNewRoom());
  await expect(controls(page).getByRole('alert')).toContainText('Останови расшифровку');
  const after = await inspect(page);
  expect(after.state.activeRoomId).toBe(before.state.activeRoomId);
  expect(after.rooms).toHaveLength(1); expect(after.state.listening).toBe(true);
});

test('DOM keyboard events do not duplicate the operating system hotkeys', async ({ page }) => {
  await load(page);
  await expect(page.getByRole('button', { name: 'Создать комнату', exact: true })).toBeEnabled();
  await page.keyboard.press('Control+Alt+n'); await page.keyboard.press('Control+Alt+m');
  await expect(controls(page)).not.toBeVisible();
  const data = await inspect(page);
  expect(data.rooms).toHaveLength(0); expect(data.calls).toEqual([]);
});

test('shortcut hints and a registration failure remain visible in settings', async ({ page }) => {
  await load(page, '?menuRequestId=7&hotkeyError=' + encodeURIComponent('Ctrl+Alt+M уже занято другим приложением.'));
  await expect(controls(page)).toBeVisible();
  await expect(controls(page).getByRole('alert')).toContainText('уже занято');
  await expect(controls(page)).toContainText('Работают, пока приложение запущено.');
  await controls(page).getByRole('button', { name: 'Закрыть настройки', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Настройки', exact: true })).toHaveAttribute('aria-keyshortcuts', 'Control+Alt+M');
  await expect(page.getByRole('button', { name: 'Новая комната', exact: true })).toHaveAttribute('aria-keyshortcuts', 'Control+Alt+N');
  await expect(page.getByRole('button', { name: 'Создать комнату', exact: true })).toHaveAttribute('aria-keyshortcuts', 'Control+Alt+N');
});
