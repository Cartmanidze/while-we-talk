import test from 'node:test';
import assert from 'node:assert/strict';
import { filterRooms, sourceLabel, historyWindow, messageBytes, MAX_MESSAGE_BYTES } from './room-helpers.ts';

function room(id, title, summary, pinned = false, updatedAt = '2026-11-16T10:00:00Z') {
  return { id, title, summary, pinned, updatedAt, createdAt: updatedAt, entryCount: 3, summaryStale: false };
}
test('room search matches all words across title and saved summary without mutating data', () => {
  const rooms = [room('a', 'Процессы Windows', 'Обсудили общую память'), room('b', 'Заметки', 'PostgreSQL')];
  assert.deepEqual(filterRooms(rooms, '  WINDOWS   память ').map(item => item.id), ['a']);
  assert.deepEqual(rooms.map(item => item.id), ['a', 'b']);
});
test('pinned rooms precede recent rooms and latest updates sort first', () => {
  const rooms = [room('old', 'Раньше', '', false), room('new', 'Позже', '', false, '2026-11-17T10:00:00Z'), room('pin', 'Важное', '', true, '2026-11-01T10:00:00Z')];
  assert.deepEqual(filterRooms(rooms, '').map(item => item.id), ['pin', 'new', 'old']);
});
test('history labels distinguish typed notes and model answers from audio', () => {
  assert.equal(sourceLabel('microphone'), 'Микрофон');
  assert.equal(sourceLabel('system'), 'Системный звук');
  assert.equal(sourceLabel('user'), 'Текст');
  assert.equal(sourceLabel('assistant'), 'Ответ модели');
});
test('history pagination requests at most 200 earlier records and ends at zero', () => {
  assert.deepEqual(historyWindow(730, 200), { remaining: 530, nextPage: 200 });
  assert.deepEqual(historyWindow(350, 200), { remaining: 150, nextPage: 150 });
  assert.deepEqual(historyWindow(80, 80), { remaining: 0, nextPage: 0 });
  assert.deepEqual(historyWindow(0, 0), { remaining: 0, nextPage: 0 });
});
test('manual message limit counts UTF-8 bytes and allows long context', () => {
  assert.equal(messageBytes('Привет'), 12);
  assert.equal(messageBytes('😀'), 4);
  assert.ok(messageBytes('a'.repeat(250001)) < MAX_MESSAGE_BYTES);
  assert.ok(messageBytes('я'.repeat(500001)) > MAX_MESSAGE_BYTES);
});
