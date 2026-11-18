import test from 'node:test';
import assert from 'node:assert/strict';
import { connectionPhase } from './provider-connection.ts';
import { providerReady } from './ui-state.ts';

const account = change => ({ connected: true, loading: false, authPending: false, models: [{ id: 'model', name: 'Model' }], selectedModel: 'model', status: '', budgetConfirmed: true, catalogSource: null, ...change });

test('ChatGPT and Claude require an explicit subscription confirmation before answering', () => {
  for (const provider of ['chatgpt', 'claude']) {
    const unconfirmed = account({ budgetConfirmed: false });
    assert.equal(providerReady(provider, unconfirmed), false);
    assert.equal(connectionPhase(provider, unconfirmed).key, 'limits');
    assert.equal(providerReady(provider, account()), true);
    assert.equal(providerReady(provider, account({ connected: false })), false);
  }
});
test('a loaded Antigravity catalog is usable but never presented as a verified connection', () => {
  const catalog = account({ connected: false, budgetConfirmed: false });
  assert.equal(providerReady('antigravity', catalog), true);
  assert.equal(connectionPhase('antigravity', catalog).key, 'catalog');
  assert.notEqual(connectionPhase('antigravity', catalog).label, 'Готово');
});
test('a pending login or removed model cannot be used for an answer', () => {
  for (const provider of ['chatgpt', 'antigravity', 'claude']) {
    assert.equal(providerReady(provider, account({ authPending: true })), false);
    assert.equal(connectionPhase(provider, account({ authPending: true })).key, 'auth');
    assert.equal(providerReady(provider, account({ loading: true })), false);
    assert.equal(providerReady(provider, account({ selectedModel: 'removed' })), false);
    assert.equal(connectionPhase(provider, account({ models: [], selectedModel: null })).key, 'model');
  }
});
test('one service waiting for login does not change another service readiness', () => {
  const connections = { chatgpt: account({ authPending: true }), antigravity: account(), claude: account() };
  assert.equal(providerReady('chatgpt', connections.chatgpt), false);
  assert.equal(providerReady('antigravity', connections.antigravity), true);
  assert.equal(providerReady('claude', connections.claude), true);
  assert.equal(connections.chatgpt.authPending, true);
});
