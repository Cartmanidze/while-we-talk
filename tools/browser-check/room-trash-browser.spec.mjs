import { test, expect } from '@playwright/test';

const externalRequests = new WeakMap();
test.beforeEach(async ({ page }) => {
  const attempted = []; externalRequests.set(page, attempted);
  await page.route('**/*', route => {
    const url = new URL(route.request().url());
    if (url.hostname === '127.0.0.1') return route.continue();
    attempted.push(url.origin); return route.abort();
  });
  await page.goto('/tools/browser-check/harness.html');
  await expect(page.getByRole('button', { name: 'Создать комнату', exact: true })).toBeEnabled();
});
test.afterEach(async ({ page }) => { expect(externalRequests.get(page)).toEqual([]); });

const inspect = page => page.evaluate(() => window.__SOBESEDNIK_TEST__.inspect());
async function createRoom(page, title) {
  await page.getByRole('button', { name: 'Новая комната', exact: true }).click();
  await page.getByRole('button', { name: 'Переименовать комнату', exact: true }).click();
  await page.getByRole('textbox', { name: 'Название комнаты', exact: true }).fill(title);
  await page.getByRole('button', { name: 'Сохранить название', exact: true }).click();
  await expect(page.getByRole('heading', { name: title, exact: true })).toBeVisible();
  return (await inspect(page)).state.activeRoomId;
}
async function addMessage(page, text) {
  await page.getByRole('textbox', { name: 'Вопрос или заметка' }).fill(text);
  await page.getByRole('button', { name: 'Сохранить реплику', exact: true }).click();
  await expect(page.getByRole('log', { name: 'История комнаты' })).toContainText(text);
}
async function requestActiveDelete(page) {
  await page.getByRole('button', { name: 'Удалить комнату', exact: true }).click();
  const confirmation = page.getByRole('region', { name: 'Удаление комнаты', exact: true });
  await expect(confirmation).toBeVisible(); return confirmation;
}
async function confirmDelete(confirmation) {
  await confirmation.getByRole('button', { name: 'Переместить в корзину', exact: true }).click();
}

test('active room moves to trash only after explicit confirmation and clears visible history', async ({ page }) => {
  const id = await createRoom(page, 'Комната с историей');
  await addMessage(page, 'Первая сохранённая реплика.'); await addMessage(page, 'Вторая сохранённая реплика.');
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.setSummary('Сохранённый итог комнаты.'));
  const before = (await inspect(page)).rooms[0];
  const confirmation = await requestActiveDelete(page);
  expect((await inspect(page)).calls.some(call => call.command === 'trash_room')).toBe(false);
  expect((await inspect(page)).rooms).toHaveLength(1);
  await confirmDelete(confirmation);
  await expect(page.getByRole('button', { name: 'Вернуть', exact: true })).toBeVisible();
  const after = await inspect(page);
  expect(after.state.activeRoomId).toBeNull(); expect(after.state.transcript).toEqual([]); expect(after.state.roomSummary).toBe('');
  expect(after.rooms).toEqual([]); expect(after.trashed[0]).toEqual(before);
  expect(after.state.trashRooms[0]).toMatchObject({ id, title: 'Комната с историей', entryCount: 2 });
  expect(after.calls.filter(call => call.command === 'trash_room')).toHaveLength(1);
  expect(after.calls.some(call => ['request_answer', 'summarize_room', 'start_listening'].includes(call.command))).toBe(false);
});

test('undo restores complete entries and summary without opening the room automatically', async ({ page }) => {
  const id = await createRoom(page, 'Возвращаемый разговор');
  await addMessage(page, 'Точная формулировка до удаления.');
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.setSummary('## Итог\n\nДоговорённость сохранена.'));
  const before = (await inspect(page)).rooms[0];
  await confirmDelete(await requestActiveDelete(page));
  await page.getByRole('button', { name: 'Вернуть', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Открыть комнату «Возвращаемый разговор»', exact: true })).toBeVisible();
  const after = await inspect(page);
  expect(after.rooms).toEqual([before]); expect(after.trashed).toEqual([]); expect(after.state.trashRooms).toEqual([]);
  expect(after.state.activeRoomId).toBeNull(); expect(after.state.transcript).toEqual([]); expect(after.state.deletionNotice).toBeNull();
  await page.getByRole('button', { name: 'Открыть комнату «Возвращаемый разговор»', exact: true }).click();
  await expect(page.getByRole('log')).toContainText('Точная формулировка до удаления.');
  await page.getByRole('tab', { name: 'Итог', exact: true }).click();
  await expect(page.getByRole('tabpanel', { name: 'Итог', exact: true })).toContainText('Договорённость сохранена.');
  expect((await inspect(page)).state.activeRoomId).toBe(id);
});

test('deleting an inactive room preserves the current room and its unsaved draft', async ({ page }) => {
  const first = await createRoom(page, 'Старая комната'); await addMessage(page, 'История старой комнаты.');
  const second = await createRoom(page, 'Текущая комната'); await addMessage(page, 'Текущий разговор продолжается.');
  const input = page.getByRole('textbox', { name: 'Вопрос или заметка' });
  await input.fill('Несохранённая заметка текущей комнаты');
  const currentBefore = (await inspect(page)).rooms.find(room => room.info.id === second);
  await page.getByRole('button', { name: 'Удалить комнату «Старая комната»', exact: true }).click();
  await confirmDelete(page.getByRole('region', { name: 'Удаление комнаты', exact: true }));
  await expect(page.getByRole('button', { name: 'Открыть комнату «Старая комната»', exact: true })).toHaveCount(0);
  await expect(page.getByRole('heading', { name: 'Текущая комната', exact: true })).toBeVisible();
  await expect(input).toHaveValue('Несохранённая заметка текущей комнаты');
  const after = await inspect(page);
  expect(after.state.activeRoomId).toBe(second); expect(after.rooms).toEqual([currentBefore]); expect(after.trashed[0].info.id).toBe(first);
  await expect(page.getByRole('log')).toContainText('Текущий разговор продолжается.');
});

test('cancel and a failed delete keep all room data and allow a deliberate retry', async ({ page }) => {
  await createRoom(page, 'Комната без потерь'); await addMessage(page, 'Эта история должна остаться.');
  const before = await inspect(page);
  const confirmation = await requestActiveDelete(page);
  await confirmation.getByRole('button', { name: 'Отмена', exact: true }).click();
  await expect(confirmation).toHaveCount(0);
  expect((await inspect(page)).calls.some(call => call.command === 'trash_room')).toBe(false);
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.failNext('trash_room'));
  await confirmDelete(await requestActiveDelete(page));
  await expect(page.getByRole('alert')).toContainText('операция не удалась');
  const failed = await inspect(page);
  expect(failed.rooms).toEqual(before.rooms); expect(failed.trashed).toEqual([]); expect(failed.state.activeRoomId).toBe(before.state.activeRoomId);
  await expect(page.getByRole('region', { name: 'Удаление комнаты', exact: true })).toBeVisible();
  await confirmDelete(page.getByRole('region', { name: 'Удаление комнаты', exact: true }));
  await expect(page.getByRole('button', { name: 'Вернуть', exact: true })).toBeVisible();
  expect((await inspect(page)).trashed).toHaveLength(1);
});

test('trash dialog keeps a failed restore retryable and restored rooms remain closed', async ({ page }) => {
  await createRoom(page, 'Диалог корзины'); await addMessage(page, 'Текст для восстановления через корзину.');
  const original = (await inspect(page)).rooms[0];
  await confirmDelete(await requestActiveDelete(page));
  await page.getByRole('button', { name: /^Корзина/ }).click();
  const dialog = page.getByRole('dialog', { name: 'Корзина', exact: true }); await expect(dialog).toBeVisible();
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.failNext('restore_room'));
  await dialog.getByRole('button', { name: 'Восстановить', exact: true }).click();
  await expect(dialog.getByRole('alert')).toContainText('операция не удалась');
  expect((await inspect(page)).trashed).toEqual([original]); expect((await inspect(page)).rooms).toEqual([]);
  await expect(dialog.getByRole('button', { name: 'Восстановить', exact: true })).toBeEnabled();
  await dialog.getByRole('button', { name: 'Восстановить', exact: true }).click();
  await expect(dialog.getByText('Корзина пуста', { exact: true })).toBeVisible();
  const after = await inspect(page);
  expect(after.rooms).toEqual([original]); expect(after.state.activeRoomId).toBeNull(); expect(after.state.transcript).toEqual([]);
  expect(after.calls.filter(call => call.command === 'restore_room')).toHaveLength(2);
  await dialog.getByRole('button', { name: 'Закрыть корзину', exact: true }).click();
  await expect(dialog).not.toBeVisible();
});

test('a search snapshot completed after deletion cannot bring its room card back', async ({ page }) => {
  await createRoom(page, 'Комната позднего поиска'); await addMessage(page, 'Уникальныймаркер находится в этой комнате.');
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.delaySearch('уникальныймаркер', 1500));
  await page.getByRole('searchbox', { name: 'Поиск по комнатам и всей истории' }).fill('уникальныймаркер');
  await expect.poll(async () => (await inspect(page)).calls.some(call => call.command === 'search_rooms')).toBe(true);
  await confirmDelete(await requestActiveDelete(page));
  await expect(page.getByRole('button', { name: 'Вернуть', exact: true })).toBeVisible();
  await expect(page.getByText('Ничего не нашлось. Попробуй другое слово.', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Открыть комнату «Комната позднего поиска»', exact: true })).toHaveCount(0);
  const after = await inspect(page);
  expect(after.rooms).toEqual([]); expect(after.trashed).toHaveLength(1);
  expect(after.calls.filter(call => call.command === 'search_rooms')).toHaveLength(1);
});
