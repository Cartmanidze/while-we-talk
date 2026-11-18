import test from 'node:test';
import assert from 'node:assert/strict';
import { WindowAppearanceController } from './window-appearance-controller.ts';

const initial = { hideFromCapture: true, windowOpacity: 100 };
const value = (windowOpacity, hideFromCapture = true) => ({ hideFromCapture, windowOpacity });
const tick = () => new Promise(resolve => queueMicrotask(resolve));
function deferred() { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; }

test('moving the slider stays local until release and sends only its last value', async () => {
  const calls = [], states = [];
  const editor = new WindowAppearanceController(initial, async next => { calls.push(next); return true; }, state => states.push(state));
  editor.edit(value(60)); editor.edit(value(45)); editor.edit(value(70));
  assert.deepEqual(calls, []);
  assert.equal(editor.snapshot().windowOpacity, 70);
  await editor.commit();
  assert.deepEqual(calls, [value(70)]);
  assert.equal(editor.snapshot().saving, false);
  assert.equal(states.at(-1).windowOpacity, 70);
});

test('slow requests serialize and retain only the latest committed change', async () => {
  const first = deferred(), second = deferred(), calls = [];
  const editor = new WindowAppearanceController(initial, next => { calls.push(next); return calls.length === 1 ? first.promise : second.promise; }, () => {});
  editor.edit(value(60)); const completion = editor.commit();
  editor.edit(value(50)); void editor.commit();
  editor.edit(value(40, false)); void editor.commit();
  editor.sync(initial);
  assert.deepEqual(calls, [value(60)]);
  assert.equal(editor.snapshot().windowOpacity, 40);
  first.resolve(true); await tick(); await tick();
  assert.deepEqual(calls, [value(60), value(40, false)]);
  second.resolve(true); await completion;
  editor.sync(value(60));
  assert.deepEqual(editor.snapshot(), { ...value(40, false), saving: false });
  editor.sync(value(40, false));
  assert.equal(editor.snapshot().windowOpacity, 40);
});

test('pointer release and blur do not submit the same pending request twice', async () => {
  const response = deferred(), calls = [];
  const editor = new WindowAppearanceController(initial, next => { calls.push(next); return response.promise; }, () => {});
  editor.edit(value(55)); const completion = editor.commit();
  void editor.commit(); void editor.commit();
  response.resolve(true); await completion;
  assert.deepEqual(calls, [value(55)]);
});

test('a rejected setting rolls back visibly and explicit retry can reapply unchanged values', async () => {
  const calls = [];
  const editor = new WindowAppearanceController(initial, async next => { calls.push(next); return calls.length > 1; }, () => {});
  editor.edit(value(30, false)); await editor.commit();
  assert.deepEqual(editor.snapshot(), { ...initial, saving: false });
  await editor.commit(true);
  assert.deepEqual(calls, [value(30, false), initial]);
});

test('a failed older request does not discard a newer reset to full opacity', async () => {
  const first = deferred(), calls = [];
  const editor = new WindowAppearanceController(value(80), next => { calls.push(next); return calls.length === 1 ? first.promise : Promise.resolve(true); }, () => {});
  editor.edit(value(40)); const completion = editor.commit();
  editor.edit(value(100)); void editor.commit();
  first.resolve(false); await completion;
  assert.deepEqual(calls, [value(40), value(100)]);
  assert.deepEqual(editor.snapshot(), { ...value(100), saving: false });
});

test('invalid transparency is rejected before invoking native code', async () => {
  const calls = [];
  const editor = new WindowAppearanceController(initial, async next => { calls.push(next); return true; }, () => {});
  for (const opacity of [0, 29, 101, 50.5, NaN]) assert.throws(() => editor.edit(value(opacity)));
  assert.deepEqual(calls, []);
  assert.deepEqual(editor.snapshot(), { ...initial, saving: false });
});
