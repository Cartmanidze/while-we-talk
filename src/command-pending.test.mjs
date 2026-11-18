import test from 'node:test';
import assert from 'node:assert/strict';
import { commandKey, commandPending } from './command-pending.ts';

test('independent service operations have distinct pending identities', () => {
  for (const command of ['refresh_models', 'select_model', 'connect_provider', 'cancel_provider_login']) {
    const keys = ['chatgpt', 'antigravity', 'claude'].map(provider => commandKey(command, { provider }));
    assert.equal(new Set(keys).size, 3);
    assert.equal(commandKey(command, { provider: 'claude' }), `${command}:claude`);
  }
});
test('one service does not block another and legacy pending names remain understood', () => {
  const pending = new Set(['refresh_models:chatgpt']);
  assert.equal(commandPending(pending, 'refresh_models', 'chatgpt'), true);
  assert.equal(commandPending(pending, 'refresh_models', 'claude'), false);
  assert.equal(commandPending(pending, 'refresh_models'), true);
  assert.equal(commandPending(new Set(['refresh_models']), 'refresh_models', 'claude'), true);
});
test('ordinary commands and the standalone ChatGPT login retain their identity', () => {
  assert.equal(commandKey('create_room'), 'create_room');
  assert.equal(commandKey('update_settings', { settings: {} }), 'update_settings');
  assert.equal(commandKey('connect_chatgpt'), 'connect_chatgpt');
});
