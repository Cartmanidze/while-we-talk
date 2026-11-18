import test, { after } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createServer } from 'vite';
import react from '@vitejs/plugin-react';

const server = await createServer({ configFile: false, cacheDir: 'node_modules/.vite-test-connections', plugins: [react()], server: { middlewareMode: true, hmr: false, watch: null }, appType: 'custom', logLevel: 'silent' });
after(() => server.close());
const { ConnectionsDialog } = await server.ssrLoadModule('/src/connections-dialog.tsx');
const { ConnectionCard } = await server.ssrLoadModule('/src/connection-card.tsx');
const { emptyState } = await server.ssrLoadModule('/src/types.ts');
function state() {
  const value = structuredClone(emptyState);
  value.settings.provider = 'antigravity';
  for (const [id, name] of [['chatgpt', 'ChatGPT catalog model'], ['antigravity', 'Antigravity catalog model'], ['claude', 'Claude catalog model']]) {
    value.providers[id] = { connected: true, loading: false, authPending: false, budgetConfirmed: true, status: 'Подключено', selectedModel: `${id}-from-catalog`, models: [{ id: `${id}-from-catalog`, name }], catalogSource: 'Официальный клиент' };
  }
  return value;
}
function card(id, value, extra = {}) {
  let calls = 0;
  const html = renderToStaticMarkup(createElement(ConnectionCard, { id, state: value, connected: true, pending: new Set(), run: async () => { calls++; return true; }, onChange: () => { calls++; }, ...extra }));
  return { html, calls };
}

test('three independent connections render with exactly one service selected and no automatic operation', () => {
  const value = state(); let calls = 0;
  const before = structuredClone(value.providers);
  const html = renderToStaticMarkup(createElement(ConnectionsDialog, { open: true, state: value, connected: true, pending: new Set(), run: async () => { calls++; return true; }, error: null, onDismissError: () => {}, onChange: () => { calls++; }, onClose: () => {}, onComponents: () => {} }));
  for (const provider of ['chatgpt', 'antigravity', 'claude']) assert.match(html, new RegExp(`id="connection-${provider}"`));
  assert.equal((html.match(/aria-pressed="true"/g) ?? []).length, 1);
  assert.equal((html.match(/connection-badge ready/g) ?? []).length, 3);
  for (const label of ['ChatGPT catalog model', 'Antigravity catalog model', 'Claude catalog model']) assert.match(html, new RegExp(label));
  assert.equal(calls, 0); assert.deepEqual(value.providers, before);
  assert.doesNotMatch(html, /type="password"|type="email"/);
});

test('subscription gates remain explicit and unchecked until the user confirms them', () => {
  const value = state(); value.providers.chatgpt.budgetConfirmed = false; value.providers.claude.budgetConfirmed = false;
  const chatgpt = card('chatgpt', value); const claude = card('claude', value);
  assert.match(chatgpt.html, /Лимит приложения 30%, платные кредиты выключены/);
  assert.match(claude.html, /Extra usage выключено в моём аккаунте Claude/);
  for (const result of [chatgpt, claude]) {
    assert.match(result.html, /type="checkbox"/); assert.doesNotMatch(result.html, /checked=""/);
    assert.match(result.html, /aria-pressed="false" disabled=""/); assert.equal(result.calls, 0);
  }
});

test('Antigravity waiting for Desktop login exposes a check action and never claims embedded OAuth', () => {
  const value = state(); Object.assign(value.providers.antigravity, { connected: false, authPending: true, loading: false });
  const result = card('antigravity', value);
  assert.match(result.html, /Antigravity Desktop/); assert.match(result.html, /Отменить вход/);
  assert.match(result.html, /<button class="text-button" type="button">[^]*?Проверить вход<\/button>/);
  assert.doesNotMatch(result.html, /connection-badge ready/); assert.equal(result.calls, 0);
});

test('another service pending request does not disable the ready Claude model picker', () => {
  const result = card('claude', state(), { pending: new Set(['refresh_models:chatgpt', 'connect_provider:antigravity']) });
  assert.match(result.html, /<select id="connection-model-claude">/);
  assert.match(result.html, /aria-pressed="false">Выбрать для ответов/);
});
