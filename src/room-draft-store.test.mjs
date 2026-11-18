import test from 'node:test';
import assert from 'node:assert/strict';
import { RoomDraftStore } from './room-draft-store.ts';

function storage() {
  const values = new Map();
  return { getItem: key => values.get(key) ?? null, setItem: (key, value) => values.set(key, value), removeItem: key => values.delete(key) };
}
test('room drafts survive component/store remounts and remain isolated', () => {
  const saved = storage();
  const first = new RoomDraftStore(() => saved);
  first.write('room-a', 'Текст A');
  first.write('room-b', 'Текст B');
  const remounted = new RoomDraftStore(() => saved);
  assert.deepEqual(remounted.read('room-a'), { text: 'Текст A', persisted: true });
  assert.deepEqual(remounted.read('room-b'), { text: 'Текст B', persisted: true });
  remounted.clear('room-a');
  assert.equal(remounted.read('room-a').text, '');
  assert.equal(remounted.read('room-b').text, 'Текст B');
});
test('a completed save cannot erase a newer edit or another room draft', () => {
  const store = new RoomDraftStore(storage);
  store.write('a', 'Отправлено');
  store.write('b', 'Другой разговор');
  store.write('a', 'Новая правка');
  assert.equal(store.clearIfUnchanged('a', 'Отправлено'), false);
  assert.equal(store.read('a').text, 'Новая правка');
  assert.equal(store.clearIfUnchanged('a', 'Новая правка'), true);
  assert.equal(store.read('a').text, '');
  assert.equal(store.read('b').text, 'Другой разговор');
});
test('unavailable session storage keeps a visible memory-only draft', () => {
  const store = new RoomDraftStore(() => { throw new Error('storage denied'); });
  store.write('a', 'Не потерять');
  assert.deepEqual(store.read('a'), { text: 'Не потерять', persisted: false });
  assert.strictEqual(store.read('a'), store.read('a'));
});
test('draft subscriptions notify only their own room', () => {
  const store = new RoomDraftStore(storage);
  let calls = 0;
  const unsubscribe = store.subscribe('a', () => calls++);
  store.write('b', 'другая');
  assert.equal(calls, 0);
  store.write('a', 'своя');
  assert.equal(calls, 1);
  unsubscribe();
  store.clear('a');
  assert.equal(calls, 1);
});
