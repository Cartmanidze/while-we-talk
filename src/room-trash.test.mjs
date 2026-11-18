import test, { after } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createServer } from 'vite';
import react from '@vitejs/plugin-react';
import { availableSearchHits } from './room-helpers.ts';

const server = await createServer({ configFile: false, cacheDir: 'node_modules/.vite-test-trash', plugins: [react()], server: { middlewareMode: true, hmr: false, watch: null }, appType: 'custom', logLevel: 'silent' });
after(() => server.close());
const { RoomDeleteConfirm } = await server.ssrLoadModule('/src/room-delete-confirm.tsx');
const { RoomTrashDialog } = await server.ssrLoadModule('/src/room-trash-dialog.tsx');

test('deletion requires a separate confirmation for the complete room and never executes on render', () => {
  let calls = 0;
  const html = renderToStaticMarkup(createElement(RoomDeleteConfirm, { room: { id: 'room-a', title: 'Разговор <img src=x>', entryCount: 12 }, locked: false, deleting: false, onDelete: () => { calls++; }, onCancel: () => {} }));
  assert.match(html, /в корзину\?/); assert.match(html, /можно вернуть вместе с историей и итогом/);
  assert.match(html, /Записей в комнате: 12/); assert.match(html, /Переместить в корзину/); assert.match(html, /Отмена/);
  assert.doesNotMatch(html, /<img/); assert.equal(calls, 0);
});
test('busy or listening state disables deletion and restoring', () => {
  const deletion = renderToStaticMarkup(createElement(RoomDeleteConfirm, { room: { id: 'a', title: 'A', entryCount: 1 }, locked: true, deleting: false, onDelete: () => {}, onCancel: () => {} }));
  assert.match(deletion, /disabled=""[^>]*>Переместить в корзину/);
  const trash = renderToStaticMarkup(createElement(RoomTrashDialog, { open: true, rooms: [{ id: 'a', title: 'A', deletedAt: '2026-11-17T01:00:00Z', entryCount: null }], locked: true, lockedReason: 'Останови расшифровку.', restoring: false, error: null, onDismissError: () => {}, onRestore: async () => true, onClose: () => {} }));
  assert.match(trash, /Не удалось посчитать записи/); assert.match(trash, /disabled=""[^>]*>Восстановить/);
  assert.doesNotMatch(trash, /Удалить навсегда|Очистить корзину/);
});
test('a late search result cannot resurrect a deleted room even when the trash list refresh failed', () => {
  const hits = ['deleted', 'healthy'].map(id => ({ room: { id }, snippet: id, entryId: 'one' }));
  const before = structuredClone(hits);
  assert.deepEqual(availableSearchHits(hits, [{ id: 'deleted' }], null).map(hit => hit.room.id), ['healthy']);
  assert.deepEqual(availableSearchHits(hits, [], 'deleted').map(hit => hit.room.id), ['healthy']);
  assert.deepEqual(availableSearchHits(hits, [], null).map(hit => hit.room.id), ['deleted', 'healthy']);
  assert.deepEqual(hits, before);
});
