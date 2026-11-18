import test, { after } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createServer } from 'vite';
import react from '@vitejs/plugin-react';

const server = await createServer({ configFile: false, cacheDir: 'node_modules/.vite-test-recovery', plugins: [react()], server: { middlewareMode: true, hmr: false, watch: null }, appType: 'custom', logLevel: 'silent' });
after(() => server.close());
const { RoomRecoveryPanel } = await server.ssrLoadModule('/src/room-recovery-panel.tsx');
const backup = { id: 'room', title: 'Рабочая версия', updatedAt: '2026-11-17T01:00:00Z', entryCount: 12 };
test('a recoverable room only offers the first confirmation step and performs no action', () => {
  let calls = 0;
  const html = renderToStaticMarkup(createElement(RoomRecoveryPanel, { recoveries: [{ id: 'room', available: true, primaryError: 'C:/rooms/room.json damaged', backupError: null, backup }], notice: null, locked: false, recovering: false, onRecover: async () => { calls++; return true; } }));
  assert.match(html, /Рабочая версия/);
  assert.match(html, /Восстановить из копии/);
  assert.doesNotMatch(html, /recovery-confirm-button/);
  assert.equal(calls, 0);
});
test('unavailable recovery exposes its path error safely without a restore button', () => {
  const html = renderToStaticMarkup(createElement(RoomRecoveryPanel, { recoveries: [{ id: 'room', available: false, primaryError: 'C:/rooms/room.json', backupError: 'C:/rooms/room.bak отсутствует <img src=x>', backup: null }], notice: null, locked: false, recovering: false, onRecover: async () => false }));
  assert.match(html, /Не удалось восстановить историю комнаты/);
  assert.match(html, /C:\/rooms\/room.bak/);
  assert.match(html, /&lt;img/);
  assert.doesNotMatch(html, /<img|Восстановить из копии/);
});
