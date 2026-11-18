import test from 'node:test';
import assert from 'node:assert/strict';
import { RoomSearchController } from './room-search-controller.ts';

const tick = () => new Promise(resolve => queueMicrotask(resolve));
const hit = id => ({ room: { id, title: id }, snippet: `found ${id}`, entryId: `entry-${id}` });
function deferred() { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); promise.catch(() => {}); return { promise, resolve, reject }; }

test('typing is debounced for 250 ms and only the latest query is sent', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const calls = [], states = [];
  const search = new RoomSearchController(async query => { calls.push(query); return [hit(query)]; }, state => states.push(state));
  search.setQuery('first'); t.mock.timers.tick(249);
  assert.deepEqual(calls, []);
  search.setQuery('second'); t.mock.timers.tick(250); await tick();
  assert.deepEqual(calls, ['second']);
  assert.equal(states.at(-1)?.hits[0].room.id, 'second');
  search.cancel();
});
test('an older success or failure cannot replace a newer result', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  for (const outcome of ['success', 'failure']) {
    const old = deferred(), recent = deferred(), states = [];
    const search = new RoomSearchController(query => query === 'old' ? old.promise : recent.promise, state => states.push(state));
    search.setQuery('old'); t.mock.timers.tick(250);
    search.setQuery('recent'); t.mock.timers.tick(250);
    recent.resolve([hit('recent')]); await tick();
    if (outcome === 'success') old.resolve([hit('old')]); else old.reject(new Error('late error'));
    await tick();
    assert.equal(states.at(-1)?.query, 'recent');
    assert.equal(states.at(-1)?.error, null);
    assert.equal(states.at(-1)?.hits[0].room.id, 'recent');
    search.cancel();
  }
});
test('clearing or unmounting invalidates an in-flight query and overlong input stays local', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const wait = deferred(), states = [];
  let calls = 0;
  const search = new RoomSearchController(() => { calls++; return wait.promise; }, state => states.push(state));
  search.setQuery('text'); t.mock.timers.tick(250);
  search.setQuery(''); wait.resolve([hit('stale')]); await tick();
  assert.deepEqual(states.at(-1), { query: '', hits: [], loading: false, error: null });
  search.setQuery('x'.repeat(201)); t.mock.timers.tick(250);
  assert.equal(calls, 1);
  assert.match(states.at(-1)?.error ?? '', /200/);
  search.cancel();
});
