import test from 'node:test';
import assert from 'node:assert/strict';
import { nextRoomTab } from './room-tabs.ts';

test('horizontal arrows move and wrap across the two room tabs', () => {
  assert.equal(nextRoomTab('conversation', 'ArrowRight'), 'summary');
  assert.equal(nextRoomTab('summary', 'ArrowRight'), 'conversation');
  assert.equal(nextRoomTab('conversation', 'ArrowLeft'), 'summary');
  assert.equal(nextRoomTab('summary', 'ArrowLeft'), 'conversation');
});
test('Home and End select endpoints while unrelated keys retain browser behavior', () => {
  assert.equal(nextRoomTab('summary', 'Home'), 'conversation');
  assert.equal(nextRoomTab('conversation', 'End'), 'summary');
  assert.equal(nextRoomTab('conversation', 'Enter'), null);
  assert.equal(nextRoomTab('summary', 'Tab'), null);
});
