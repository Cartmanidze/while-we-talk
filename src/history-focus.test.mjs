import test from 'node:test';
import assert from 'node:assert/strict';
import { HistoryFocus } from './history-focus.ts';

test('a search entry focuses once and repeated state ticks do not scroll it again', () => {
  const focus = new HistoryFocus();
  assert.equal(focus.next('room', 'entry', 1, true, false, ['entry']), 'entry');
  assert.equal(focus.next('room', 'entry', 1, true, false, ['entry']), null);
  assert.equal(focus.next('room', 'entry', 2, true, false, ['entry']), 'entry');
});
test('focus waits until the correct room data is ready and visible', () => {
  const focus = new HistoryFocus();
  assert.equal(focus.next('room', 'entry', 1, true, true, ['entry']), null);
  assert.equal(focus.next('room', 'entry', 1, true, false, []), null);
  assert.equal(focus.next('room', 'entry', 1, false, false, ['entry']), null);
  assert.equal(focus.next('room', 'entry', 1, true, false, ['entry']), 'entry');
  assert.equal(focus.next('room', null, 2, true, false, ['entry']), null);
  assert.equal(focus.next('room', 'entry', 2, true, false, ['entry']), 'entry');
});
