import { test, expect } from '@playwright/test';

const remoteRequests = new WeakMap();

test.beforeEach(async ({ page }) => {
  const attempted = [];
  remoteRequests.set(page, attempted);
  await page.route('**/*', route => {
    const url = new URL(route.request().url());
    if (url.hostname === '127.0.0.1') return route.continue();
    attempted.push(url.origin);
    return route.abort();
  });
  await page.goto('/tools/browser-check/harness.html');
  await expect(page.getByRole('button', { name: 'Создать комнату', exact: true })).toBeEnabled();
});

test.afterEach(async ({ page }) => {
  expect(remoteRequests.get(page), 'The isolated interface must not attempt external requests').toEqual([]);
});

async function inspect(page) {
  return page.evaluate(() => window.__SOBESEDNIK_TEST__.inspect());
}

async function createRoom(page) {
  await page.getByRole('button', { name: 'Создать комнату', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Вопрос или заметка' })).toBeVisible();
}

async function addNote(page) {
  await page.getByRole('textbox', { name: 'Вопрос или заметка' }).fill('Сохраним этот разговор в отдельной комнате.');
  await page.getByRole('button', { name: 'Сохранить реплику', exact: true }).click();
  await expect(page.getByRole('log', { name: 'История комнаты' })).toContainText('Сохраним этот разговор');
}

async function expectContrast(locator, label) {
  await expect(locator).toBeVisible();
  const result = await locator.evaluate(element => {
    const canvas = document.createElement('canvas');
    canvas.width = canvas.height = 1;
    const context = canvas.getContext('2d', { willReadFrequently: true });
    const rgba = color => {
      context.clearRect(0, 0, 1, 1);
      context.fillStyle = color;
      context.fillRect(0, 0, 1, 1);
      const [r, g, b, a] = context.getImageData(0, 0, 1, 1).data;
      return [r, g, b, a / 255];
    };
    const over = (front, back) => {
      const alpha = front[3] + back[3] * (1 - front[3]);
      return [0, 1, 2].map(i => (front[i] * front[3] + back[i] * back[3] * (1 - front[3])) / alpha).concat(alpha);
    };
    const layers = [];
    let opaque = false;
    for (let node = element; node; node = node.parentElement) {
      const style = getComputedStyle(node);
      // A CSS color calculation cannot measure a gradient, image, filter, or group opacity.
      // Fail explicitly instead of reporting a misleading contrast score for those surfaces.
      if (Number(style.opacity) !== 1 || style.filter !== 'none') throw new Error('Contrast measurement needs a rendered sample for opacity/filter');
      if (opaque) continue;
      if (style.backgroundImage !== 'none') throw new Error('Contrast measurement needs a rendered sample for a background image');
      const color = rgba(style.backgroundColor);
      layers.push(color);
      opaque = color[3] === 1;
    }
    let background = [255, 255, 255, 1];
    for (const layer of layers.reverse()) background = over(layer, background);
    const foreground = over(rgba(getComputedStyle(element).color), background);
    const luminance = color => {
      const linear = color.slice(0, 3).map(value => {
        const srgb = value / 255;
        return srgb <= 0.04045 ? srgb / 12.92 : ((srgb + 0.055) / 1.055) ** 2.4;
      });
      return linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722;
    };
    const lightness = [luminance(foreground), luminance(background)].sort((a, b) => a - b);
    return { ratio: (lightness[1] + 0.05) / (lightness[0] + 0.05), foreground, background };
  });
  expect(result.ratio, `${label}: ${JSON.stringify(result)}`).toBeGreaterThanOrEqual(4.5);
}

test('welcome reaches listening in exactly two actions and stop remains available', async ({ page }) => {
  await createRoom(page);
  const created = await inspect(page);
  expect(created.rooms).toHaveLength(1);
  expect(created.state.listening).toBe(false);
  expect(created.state.busy).toBe(false);
  expect(created.calls.map(call => call.command)).toEqual(['create_room']);

  await page.getByRole('button', { name: 'Начать расшифровку', exact: true }).click();
  const stop = page.getByRole('button', { name: 'Остановить расшифровку', exact: true });
  await expect(stop).toBeEnabled();
  const listening = await inspect(page);
  expect(listening.state.listening).toBe(true);
  expect(listening.calls.map(call => call.command)).toEqual(['create_room', 'start_listening']);

  await stop.click();
  await expect(page.getByRole('button', { name: 'Начать расшифровку', exact: true })).toBeEnabled();
  const stopped = await inspect(page);
  expect(stopped.state.listening).toBe(false);
  expect(stopped.calls.map(call => call.command)).toEqual(['create_room', 'start_listening', 'stop_listening']);
});

test('unavailable model has a direct connection action without starting sound or a cloud operation', async ({ page }) => {
  await createRoom(page);
  await addNote(page);
  await page.evaluate(() => window.__SOBESEDNIK_TEST__.setProviderUnavailable());
  const answer = page.getByRole('region', { name: 'Ответ', exact: true });
  const connect = answer.getByRole('button', { name: 'Подключить модель', exact: true });
  await expect(connect).toBeEnabled();
  const before = await inspect(page);
  await connect.click();
  await expect(page.getByRole('dialog', { name: 'Подключения', exact: true })).toBeVisible();
  const after = await inspect(page);
  expect(after.calls).toEqual(before.calls);
  expect(after.state.listening).toBe(false);
  expect(after.state.busy).toBe(false);
});

test('welcome and empty answer omit unavailable listening and decorative badges', async ({ page }) => {
  await expect(page.getByRole('button', { name: 'Начать расшифровку', exact: true })).toHaveCount(0);
  await expect(page.getByText('По запросу', { exact: true })).toHaveCount(0);
  await expect(page.locator('.room-welcome-mark, .answer-mark')).toHaveCount(0);
  await createRoom(page);
  await expect(page.getByRole('button', { name: 'Начать расшифровку', exact: true })).toBeEnabled();
  await expect(page.getByText('По запросу', { exact: true })).toHaveCount(0);
  await expect(page.locator('.room-welcome-mark, .answer-mark')).toHaveCount(0);
});

for (const viewport of [{ width: 1220, height: 820 }, { width: 900, height: 650 }]) {
  test(`primary actions and conversation text retain readable contrast at ${viewport.width}px`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await page.mouse.move(0, 0);
    await expectContrast(page.getByRole('button', { name: 'Создать комнату', exact: true }), 'Create room');
    await expectContrast(page.getByRole('main').getByRole('heading', { level: 1 }), 'Welcome heading');
    await expectContrast(page.locator('.no-room-state > p'), 'Welcome explanation');
    await page.screenshot({ path: testInfo.outputPath('welcome.png'), fullPage: true });

    await createRoom(page);
    await page.mouse.move(0, 0);
    await expectContrast(page.getByRole('button', { name: 'Начать расшифровку', exact: true }), 'Start listening');
    await page.getByRole('button', { name: 'Начать расшифровку', exact: true }).click();
    const stop = page.getByRole('button', { name: 'Остановить расшифровку', exact: true });
    await expect(stop).toBeEnabled();
    await page.mouse.move(0, 0);
    await expectContrast(stop, 'Stop listening');
    await stop.click();
    await addNote(page);
    await page.mouse.move(0, 0);
    const answer = page.getByRole('region', { name: 'Ответ', exact: true });
    await expect(answer.getByRole('button', { name: /^Получить ответ/ })).toBeEnabled();
    await expectContrast(answer.getByRole('button', { name: /^Получить ответ/ }).locator('span'), 'Request answer');
    await expectContrast(page.getByRole('log', { name: 'История комнаты' }).getByText('Сохраним этот разговор в отдельной комнате.', { exact: true }), 'Conversation text');
    await expectContrast(answer.locator('.empty-answer p'), 'Answer explanation');
    await page.screenshot({ path: testInfo.outputPath('room.png'), fullPage: true });

    await page.evaluate(() => window.__SOBESEDNIK_TEST__.setProviderUnavailable());
    await expectContrast(answer.getByRole('button', { name: 'Подключить модель', exact: true }), 'Connect model');
    const result = await inspect(page);
    expect(result.calls.map(call => call.command)).toEqual(['create_room', 'start_listening', 'stop_listening', 'add_message']);
  });
}
