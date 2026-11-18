import test from 'node:test';
import assert from 'node:assert/strict';
import * as ui from './ui-state.ts';
function state() {
  return { activeRoomId: 'room-a', historyTotal: 1, summaryBusy: false, audioReady: true, listening: false, busy: false,
    settings: { provider: 'chatgpt', micEnabled: true, systemEnabled: false },
    devices: { microphones: [{ id: 'mic' }], system: [] },
    transcript: [{ source: 'user', text: 'Вопрос' }],
    answer: { text: '' },
    providers: { chatgpt: { connected: true, budgetConfirmed: true, selectedModel: 'gpt', models: [{ id: 'gpt' }], loading: false }, antigravity: { connected: false, selectedModel: 'gemini', models: [{ id: 'gemini' }], loading: false } },
  };
}
test('subscription answer needs confirmed budget and an actual catalog selection', () => {
  const s = state();
  assert.equal(ui.canRequestAnswer(s), true);
  s.providers.chatgpt.budgetConfirmed = false;
  assert.equal(ui.canRequestAnswer(s), false);
  s.providers.chatgpt.budgetConfirmed = true;
  s.providers.chatgpt.selectedModel = 'removed';
  assert.equal(ui.canRequestAnswer(s), false);
});
test('listening stays local and does not require a connected model', () => {
  const s = state();
  s.providers.chatgpt.connected = false;
  assert.equal(ui.canStartListening(s), true);
  s.devices.microphones = [];
  assert.equal(ui.canStartListening(s), false);
});
test('Antigravity can make its first explicit request from its catalog', () => {
  const s = state();
  s.settings.provider = 'antigravity';
  assert.equal(ui.canRequestAnswer(s), true);
  s.busy = true;
  assert.equal(ui.canRequestAnswer(s), false);
});
test('opening the app without a room cannot start capture or request a model', () => {
  const s = state();
  s.activeRoomId = null;
  assert.equal(ui.canStartListening(s), false);
  assert.equal(ui.canRequestAnswer(s), false);
});
test('room changes are blocked during capture or either kind of generation', () => {
  const s = state();
  assert.equal(ui.canManageRooms(s), true);
  s.summaryBusy = true;
  assert.equal(ui.canManageRooms(s), false);
  s.summaryBusy = false;
  s.listening = true;
  assert.equal(ui.canManageRooms(s), false);
});
test('summary needs a room, saved history, a provider and stopped capture', () => {
  const s = state();
  assert.equal(ui.canSummarizeRoom(s), true);
  s.listening = true;
  assert.equal(ui.canSummarizeRoom(s), false);
  s.listening = false;
  s.historyTotal = 0;
  assert.equal(ui.canSummarizeRoom(s), false);
});
