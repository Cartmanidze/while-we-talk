import { test, expect } from '@playwright/test';

const outside = new WeakMap();
test.beforeEach(async ({ page }) => {
  const requests = []; outside.set(page, requests);
  await page.route('**/*', route => {
    const url = new URL(route.request().url());
    if (url.hostname === '127.0.0.1') return route.continue();
    requests.push(url.origin); return route.abort();
  });
  await page.goto('/tools/browser-check/harness.html');
  await page.getByRole('button', { name: 'Создать комнату', exact: true }).click();
});
test.afterEach(async ({ page }) => expect(outside.get(page)).toEqual([]));

const inspect = page => page.evaluate(() => window.__SOBESEDNIK_TEST__.inspect());
const dialog = page => page.getByRole('dialog', { name: 'Подключения', exact: true });
const card = (page, name) => dialog(page).getByRole('region', { name, exact: true });
async function openConnections(page) {
  await page.getByRole('button', { name: 'Настройки', exact: true }).click();
  await page.getByRole('dialog', { name: 'Настройки', exact: true }).getByRole('button', { name: 'Подключения', exact: true }).click();
  await expect(dialog(page)).toBeVisible();
}
async function closeConnections(page) { await dialog(page).getByRole('button', { name: 'Закрыть подключения', exact: true }).click(); }
async function note(page) {
  await page.getByRole('textbox', { name: 'Вопрос или заметка' }).fill('Проверка независимых подключений и выбора модели.');
  await page.getByRole('button', { name: 'Сохранить реплику', exact: true }).click();
  await expect(page.getByRole('log')).toContainText('независимых подключений');
}

test('three services connect independently and retain a single selected service', async ({ page }) => {
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.disconnectProviders());
  await openConnections(page);
  await card(page, 'ChatGPT').getByRole('button', { name: 'Войти через ChatGPT', exact: true }).click();
  await card(page, 'Claude Code').getByRole('button', { name: 'Войти через Claude Code', exact: true }).click();
  await card(page, 'Antigravity').getByRole('button', { name: 'Открыть Antigravity', exact: true }).click();
  await expect(card(page, 'Antigravity').getByRole('button', { name: 'Проверить вход', exact: true })).toBeEnabled();
  await card(page, 'Antigravity').getByRole('button', { name: 'Проверить вход', exact: true }).click();
  await expect.poll(async () => Object.values((await inspect(page)).state.providers).every(provider => provider.connected)).toBe(true);
  let data = await inspect(page);
  expect(data.state.providers.chatgpt.budgetConfirmed).toBe(false);
  expect(data.state.providers.claude.budgetConfirmed).toBe(false);
  await card(page, 'ChatGPT').getByRole('checkbox', { name: /Лимит приложения 30%/ }).click();
  await card(page, 'Claude Code').getByRole('checkbox', { name: /Extra usage/ }).click();
  await expect(dialog(page).locator('.connection-badge.ready')).toHaveCount(3);
  await expect(dialog(page).locator('button[aria-pressed="true"]')).toHaveCount(1);
  data = await inspect(page);
  expect(data.state.settings.provider).toBe('antigravity');
  expect(data.state.providers.chatgpt.budgetConfirmed).toBe(true);
  expect(data.state.providers.claude.budgetConfirmed).toBe(true);
  expect(data.calls.some(call => ['request_answer', 'summarize_room', 'start_listening'].includes(call.command))).toBe(false);
});

test('Claude billing gate blocks answering until explicit confirmation, then only the chosen model answers', async ({ page }) => {
  await note(page); await openConnections(page);
  await card(page, 'Claude Code').getByRole('button', { name: 'Войти через Claude Code', exact: true }).click();
  await expect.poll(async () => (await inspect(page)).state.providers.claude.connected).toBe(true);
  await expect(card(page, 'Claude Code').getByRole('button', { name: 'Выбрать для ответов', exact: true })).toBeDisabled();
  await closeConnections(page);
  await page.getByRole('button', { name: 'Настройки', exact: true }).click();
  const settings = page.getByRole('dialog', { name: 'Настройки', exact: true });
  await settings.getByRole('combobox', { name: 'Сервис', exact: true }).selectOption('claude');
  await settings.getByRole('button', { name: 'Закрыть настройки', exact: true }).click();
  const answer = page.getByRole('region', { name: 'Ответ', exact: true });
  await expect(answer.getByRole('button', { name: 'Подключить модель', exact: true })).toBeEnabled();
  expect((await inspect(page)).calls.some(call => call.command === 'request_answer')).toBe(false);
  await answer.getByRole('button', { name: 'Подключить модель', exact: true }).click();
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.addModel('claude', 'claude-selected-test', 'Claude выбранная модель'));
  await card(page, 'Claude Code').getByRole('combobox', { name: 'Модель Claude Code', exact: true }).selectOption('claude-selected-test');
  await card(page, 'Claude Code').getByRole('checkbox', { name: /Extra usage/ }).click();
  await expect.poll(async () => (await inspect(page)).state.providers.claude.budgetConfirmed).toBe(true);
  await closeConnections(page);
  await answer.getByRole('button', { name: /^Получить ответ/ }).click();
  await expect(answer).toContainText('контекст разговоров');
  const data = await inspect(page);
  expect(data.calls.filter(call => call.command === 'request_answer')).toHaveLength(1);
  expect(data.state.answer.model).toBe('claude-selected-test');
  expect(data.state.settings.claudeModel).toBe('claude-selected-test');
  expect(data.state.providers.antigravity.connected).toBe(true);
  expect(data.state.providers.antigravity.selectedModel).toBe('fixture-model');
});

test('cancelling one login does not cancel another service login', async ({ page }) => {
  await page.evaluate(async () => {
    await window.__SOBESEDNIK_TEST__.disconnectProviders();
    window.__SOBESEDNIK_TEST__.delayAuth('chatgpt', 2500); window.__SOBESEDNIK_TEST__.delayAuth('claude', 2500);
  });
  await openConnections(page);
  await card(page, 'ChatGPT').getByRole('button', { name: 'Войти через ChatGPT', exact: true }).click();
  await card(page, 'Claude Code').getByRole('button', { name: 'Войти через Claude Code', exact: true }).click();
  await expect(card(page, 'ChatGPT').getByRole('button', { name: 'Отменить вход', exact: true })).toBeVisible();
  await card(page, 'ChatGPT').getByRole('button', { name: 'Отменить вход', exact: true }).click();
  await expect.poll(async () => (await inspect(page)).state.providers.claude.connected).toBe(true);
  const data = await inspect(page);
  expect(data.state.providers.chatgpt.connected).toBe(false);
  expect(data.state.providers.chatgpt.authPending).toBe(false);
  expect(data.calls.filter(call => call.command === 'cancel_provider_login').map(call => call.args.provider)).toEqual(['chatgpt']);
  expect(data.state.settings.provider).toBe('antigravity');
});

test('refresh pending for one service does not block another refresh or an answer from the selected service', async ({ page }) => {
  await note(page);
  await page.evaluate(async () => {
    await window.__SOBESEDNIK_TEST__.readyProviders();
    window.__SOBESEDNIK_TEST__.delayCommand('refresh_models', 'chatgpt', 2500);
    window.__SOBESEDNIK_TEST__.delayCommand('refresh_models', 'claude', 2000);
  });
  await openConnections(page);
  await card(page, 'ChatGPT').getByRole('button', { name: 'Проверить вход', exact: true }).click();
  await expect.poll(async () => (await inspect(page)).state.providers.chatgpt.loading).toBe(true);
  await expect(card(page, 'Claude Code').getByRole('button', { name: 'Проверить вход', exact: true })).toBeEnabled();
  await card(page, 'Claude Code').getByRole('button', { name: 'Проверить вход', exact: true }).click();
  await expect.poll(async () => (await inspect(page)).calls.filter(call => call.command === 'refresh_models').length).toBe(2);
  const pending = await inspect(page);
  expect(pending.state.providers.chatgpt.loading).toBe(true); expect(pending.state.providers.claude.loading).toBe(true);
  await closeConnections(page);
  const answer = page.getByRole('region', { name: 'Ответ', exact: true });
  await expect(answer.getByRole('button', { name: /^Получить ответ/ })).toBeEnabled();
  await answer.getByRole('button', { name: /^Получить ответ/ }).click();
  await expect(answer).toContainText('контекст разговоров');
  expect((await inspect(page)).state.answer.model).toBe('antigravity-example');
});

test('cancelling Antigravity waiting state keeps an already running check visible', async ({ page }) => {
  await page.evaluate(async () => {
    await window.__SOBESEDNIK_TEST__.disconnectProviders();
    window.__SOBESEDNIK_TEST__.delayCommand('refresh_models', 'antigravity', 1800);
  });
  await openConnections(page);
  const antigravity = card(page, 'Antigravity');
  await antigravity.getByRole('button', { name: 'Открыть Antigravity', exact: true }).click();
  await antigravity.getByRole('button', { name: 'Проверить вход', exact: true }).click();
  await expect.poll(async () => (await inspect(page)).state.providers.antigravity.loading).toBe(true);
  await antigravity.getByRole('button', { name: 'Отменить вход', exact: true }).click();
  await expect.poll(async () => (await inspect(page)).state.providers.antigravity.authPending).toBe(false);
  const pending = (await inspect(page)).state.providers.antigravity;
  expect(pending.loading).toBe(true); expect(pending.status).toContain('Проверяю');
  await expect.poll(async () => (await inspect(page)).state.providers.antigravity.loading).toBe(false);
});

test('each service keeps its own selected catalog model while the active service changes', async ({ page }) => {
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.readyProviders()); await openConnections(page);
  await card(page, 'ChatGPT').getByRole('combobox', { name: 'Модель ChatGPT', exact: true }).selectOption('chatgpt-example-fast');
  await card(page, 'Claude Code').getByRole('combobox', { name: 'Модель Claude Code', exact: true }).selectOption('claude-example-fast');
  await card(page, 'Claude Code').getByRole('button', { name: 'Выбрать для ответов', exact: true }).click();
  await expect.poll(async () => (await inspect(page)).state.settings.provider).toBe('claude');
  await card(page, 'ChatGPT').getByRole('button', { name: 'Выбрать для ответов', exact: true }).click();
  const data = await inspect(page);
  expect(data.state.settings.provider).toBe('chatgpt');
  expect(data.state.providers.chatgpt.selectedModel).toBe('chatgpt-example-fast');
  expect(data.state.providers.claude.selectedModel).toBe('claude-example-fast');
  expect(Object.values(data.state.providers).every(provider => provider.connected)).toBe(true);
  await expect(dialog(page).locator('button[aria-pressed="true"]')).toHaveCount(1);
  expect(data.calls.some(call => ['request_answer', 'summarize_room', 'start_listening'].includes(call.command))).toBe(false);
});
