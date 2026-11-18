import React from 'react';
import ReactDOM from 'react-dom/client';
import { mockIPC } from '@tauri-apps/api/mocks';
import { emit } from '@tauri-apps/api/event';
import { App } from '../app';
import { emptyState, type AppState, type ProviderId, type RoomInfo, type TranscriptEntry } from '../types';
import '../styles.css';

if (!import.meta.env.DEV) throw new Error('The test harness is available only through the development server');
Object.defineProperty(window, 'isTauri', { value: true, configurable: true });
const style = document.createElement('style');
style.textContent = '.desktop-shell {height:calc(100dvh - 24px)}'; document.head.append(style);

type FixtureRoom = { info: RoomInfo; entries: TranscriptEntry[]; answer: AppState['answer'] };
let state: AppState;
let rooms: FixtureRoom[];
let trashed: FixtureRoom[];
const authGenerations = { chatgpt: 0, antigravity: 0, claude: 0 };
const calls: { command: string; args: Record<string, unknown> }[] = [];
let serial = 0;
let failCommand: string | null = null;
let generation = 0;
let menuRequestSequence = 0;
const searchDelays = new Map<string, number>();
const commandDelays = new Map<string, number>();
const authDelays = new Map<ProviderId, number>();
function reset() {
  state = structuredClone(emptyState); rooms = []; trashed = []; calls.length = 0; serial = 0; generation++;
  searchDelays.clear(); commandDelays.clear(); authDelays.clear(); failCommand = null;
  authGenerations.chatgpt++; authGenerations.antigravity++; authGenerations.claude++;
  state.audioReady = true; state.audioStatus = 'Тестовый звук · оборудование не используется';
  const initialMenuId = Number(new URLSearchParams(location.search).get('menuRequestId') ?? 0);
  Object.assign(state, { menuRequestId: Number.isSafeInteger(initialMenuId) && initialMenuId >= 0 ? initialMenuId : 0, hotkeyError: new URLSearchParams(location.search).get('hotkeyError') });
  menuRequestSequence = initialMenuId || 0;
  state.windowAppearanceApplied = true;
  state.settings.provider = 'antigravity';
  state.devices = { microphones: [{ id: 'test-mic', name: 'Тестовый микрофон' }], system: [{ id: 'test-system', name: 'Тестовый системный звук' }], defaultMicrophoneId: 'test-mic', defaultSystemId: 'test-system' };
  state.providers.antigravity = { connected: true, loading: false, authPending: false, catalogSource: null, models: [{ id: 'fixture-model', name: 'Тестовая модель' }], selectedModel: 'fixture-model', status: 'Имитация · запросов в сеть нет', budgetConfirmed: false };
}
reset();
function current() { const room = rooms.find(room => room.info.id === state.activeRoomId); if (!room) throw new Error('Комната не открыта'); return room; }
function sync() {
  state.rooms = rooms.map(room => ({ ...room.info, entryCount: room.entries.length }));
  if (state.activeRoomId) {
    const room = current(); state.roomTitle = room.info.title; state.roomSummary = room.info.summary;
    state.summaryStale = room.info.summaryStale; state.historyTotal = room.entries.length;
    state.transcript = room.entries.slice(-state.historyLimit); state.answer = room.answer;
  } else { state.transcript = []; state.historyTotal = 0; state.roomTitle = ''; state.roomSummary = ''; state.summaryStale = false; state.answer = structuredClone(emptyState.answer); state.focusEntryId = null; }
}
async function publish() { sync(); await emit('app-state', structuredClone(state)); }
function create() {
  const now = new Date().toISOString();
  const room: FixtureRoom = { info: { id: `test-${++serial}`, title: 'Новая комната', pinned: false, createdAt: now, updatedAt: now, entryCount: 0, summary: '', summaryStale: false }, entries: [], answer: structuredClone(emptyState.answer) };
  rooms.push(room); state.activeRoomId = room.info.id; state.historyLimit = 200; Object.assign(state, { menuRequestId: 0 }); return room;
}
mockIPC(async (command, payload) => {
  const args = (payload ?? {}) as Record<string, unknown>;
  if (command === 'get_state') { sync(); return structuredClone(state); }
  calls.push({ command, args: structuredClone(args) });
  if (command === failCommand) { failCommand = null; throw new Error('Тест: операция не удалась. Данные сохранены.'); }
  const commandDelay = commandDelays.get(`${command}:${String(args.provider ?? '')}`) ?? 0;
  if (commandDelay && command !== 'refresh_models') await new Promise(resolve => setTimeout(resolve, commandDelay));
  switch (command) {
    case 'create_room': create(); break;
    case 'open_room': {
      state.activeRoomId = String(args.roomId); const room = current();
      const index = room.entries.findIndex(entry => entry.id === args.entryId);
      state.historyLimit = index >= 0 ? Math.max(200, room.entries.length - index) : 200;
      Object.assign(state, { focusEntryId: index >= 0 ? String(args.entryId) : null });
      break;
    }
    case 'rename_room': current().info.title = String(args.title); break;
    case 'pin_room': { const room = rooms.find(room => room.info.id === args.roomId); if (room) room.info.pinned = Boolean(args.pinned); break; }
    case 'trash_room': {
      if (state.listening || state.busy) throw new Error('Перед удалением останови расшифровку');
      const index = rooms.findIndex(room => room.info.id === args.roomId);
      if (index < 0) throw new Error('Комната не найдена');
      const room = rooms.splice(index, 1)[0]; trashed.push(room);
      state.trashRooms.push({ id: room.info.id, title: room.info.title, deletedAt: new Date().toISOString(), entryCount: room.entries.length });
      state.deletionNotice = room.info.id;
      if (state.activeRoomId === room.info.id) state.activeRoomId = null;
      break;
    }
    case 'restore_room': {
      if (state.listening || state.busy) throw new Error('Останови расшифровку перед восстановлением');
      const index = trashed.findIndex(room => room.info.id === args.roomId);
      if (index < 0) throw new Error('Комната не найдена в корзине');
      rooms.push(trashed.splice(index, 1)[0]);
      state.trashRooms = state.trashRooms.filter(room => room.id !== args.roomId);
      if (state.deletionNotice === args.roomId) state.deletionNotice = null;
      break;
    }
    case 'add_message': {
      const room = current(); const text = String(args.text);
      room.entries.push({ id: `${Date.now()}-${room.entries.length}`, source: 'user', text, at: new Date().toISOString() });
      if (room.info.title === 'Новая комната') room.info.title = text.slice(0, 60);
      if (room.info.summary) room.info.summaryStale = true;
      break;
    }
    case 'load_more_history': {
      const first = Math.max(0, state.historyTotal - state.historyLimit);
      const before = typeof args.before === 'number' ? Math.min(first, args.before) : first;
      state.historyLimit = Math.max(200, state.historyTotal - Math.max(0, before - 200)); break;
    }
    case 'start_listening': state.listening = true; break;
    case 'stop_listening': state.listening = false; break;
    case 'update_settings': {
      const previous = state.settings;
      state.settings = { ...(args.settings as AppState['settings']), chatgptModel: previous.chatgptModel, antigravityModel: previous.antigravityModel, claudeModel: previous.claudeModel, hideFromCapture: previous.hideFromCapture, windowOpacity: previous.windowOpacity };
      break;
    }
    case 'update_window_appearance':
      state.settings.hideFromCapture = Boolean(args.hideFromCapture);
      state.settings.windowOpacity = Number(args.windowOpacity);
      state.windowAppearanceApplied = true;
      break;
    case 'select_model': {
      const id = args.provider as ProviderId; const modelId = String(args.modelId);
      if (!state.providers[id].models.some(model => model.id === modelId)) throw new Error('Модель отсутствует в тестовом каталоге');
      state.providers[id].selectedModel = modelId;
      if (id === 'chatgpt') state.settings.chatgptModel = modelId;
      else if (id === 'antigravity') state.settings.antigravityModel = modelId;
      else state.settings.claudeModel = modelId;
      break;
    }
    case 'connect_chatgpt':
    case 'connect_provider': {
      const id = command === 'connect_chatgpt' ? 'chatgpt' : args.provider as 'antigravity' | 'claude';
      const provider = state.providers[id]; const epoch = ++authGenerations[id];
      provider.authPending = true; provider.loading = id !== 'antigravity'; provider.status = 'Ожидаю вход в браузере';
      if (id !== 'antigravity') setTimeout(() => {
        if (authGenerations[id] !== epoch) return;
        Object.assign(provider, { connected: true, loading: false, authPending: false, budgetConfirmed: false, status: 'Вход выполнен · проверь подписку', models: [{ id: `${id}-test`, name: `${id === 'claude' ? 'Claude' : 'ChatGPT'} тест` }], selectedModel: `${id}-test` });
        void publish();
      }, authDelays.get(id) ?? 120);
      break;
    }
    case 'cancel_provider_login': {
      const id = args.provider as 'chatgpt' | 'antigravity' | 'claude'; authGenerations[id]++;
      const provider = state.providers[id]; provider.authPending = false;
      if (id !== 'antigravity' || !provider.loading) Object.assign(provider, { loading: false, status: 'Вход отменён' });
      break;
    }
    case 'confirm_budget': state.providers.chatgpt.budgetConfirmed = true; break;
    case 'confirm_claude_subscription': state.providers.claude.budgetConfirmed = true; break;
    case 'open_provider_usage': break;
    case 'refresh_models': {
      const id = args.provider as 'chatgpt' | 'antigravity' | 'claude';
      const provider = state.providers[id]; const waitingForDesktop = id === 'antigravity' && provider.authPending;
      provider.loading = true; provider.status = 'Проверяю подключение…'; await publish();
      if (commandDelay) await new Promise(resolve => setTimeout(resolve, commandDelay));
      if (waitingForDesktop) Object.assign(provider, { connected: true, authPending: false, models: [{ id: 'fixture-model', name: 'Тестовая модель' }], selectedModel: 'fixture-model' });
      provider.loading = false; provider.status = provider.connected ? 'Подключено' : 'Подключение ещё не подтверждено';
      break;
    }
    case 'dismiss_error': state.error = null; break;
    case 'search_rooms': {
      const query = String(args.query).toLocaleLowerCase('ru').trim();
      const words = query.split(/\s+/).filter(Boolean);
      const snapshot = structuredClone(rooms.filter(room => words.every(word => `${room.info.title} ${room.info.summary} ${room.entries.map(e => e.text).join(' ')}`.toLocaleLowerCase('ru').includes(word)))
        .map(room => { const entry = room.entries.find(e => words.some(word => e.text.toLocaleLowerCase('ru').includes(word))); return { room: room.info, snippet: entry?.text ?? room.info.summary.slice(0, 220), entryId: entry?.id ?? null }; }));
      const wait = searchDelays.get(query) ?? 5; await new Promise(resolve => setTimeout(resolve, wait));
      return snapshot;
    }
    case 'recover_room': {
      const room = create(); room.info.title = 'Восстановленная тестовая комната';
      room.entries.push({ id: 'recovered-1', source: 'user', text: 'История из резервной копии.', at: new Date().toISOString() });
      Object.assign(state, { roomRecoveries: [], recoveryNotice: 'Комната восстановлена. Повреждённый файл сохранён: test-room.corrupt. Несохранённый текст сохранён: test-room.unsaved.' });
      break;
    }
    case 'export_room': return `# ${current().info.title}\n\n${current().info.summary}\n\n${current().entries.map(e => e.text).join('\n\n')}`;
    case 'request_answer':
    case 'summarize_room': {
      state.busy = true; state.summaryBusy = command === 'summarize_room';
      const turn = ++generation; const room = current();
      const model = state.providers[state.settings.provider].selectedModel ?? 'fixture-model';
      if (!state.summaryBusy) room.answer = { ...structuredClone(emptyState.answer), streaming: true, model };
      setTimeout(() => {
        if (turn !== generation) return;
        if (command === 'summarize_room') { room.info.summary = '## Итог\n\nРешили **сохранять историю**.\n\n- Проверить перезапуск\n- Открыть старую комнату\n\n```rust\nfn main() { println!("История сохранена"); }\n```\n\n| Проверка | Статус |\n| --- | --- |\n| Комнаты | Готово |'; room.info.summaryStale = false; }
        else { room.answer = { text: 'Комнаты разделяют **контекст разговоров**.\n\n```rust\nlet room = "example";\n```', model, streaming: false, firstTokenMs: 100, totalMs: 400 }; room.entries.push({ id: `${Date.now()}`, source: 'assistant', text: room.answer.text, at: new Date().toISOString() }); }
        state.busy = false; state.summaryBusy = false; void publish();
      }, 450);
      break;
    }
    case 'cancel_answer': generation++; state.busy = false; state.summaryBusy = false; if (state.activeRoomId) current().answer.streaming = false; break;
    default: throw new Error(`Unimplemented test command: ${command}`);
  }
  await publish(); return null;
}, { shouldMockEvents: true });
Object.defineProperty(window, '__SOBESEDNIK_TEST__', { value: {
  inspect: () => structuredClone({ state, rooms, trashed, calls }),
  reset: async () => { reset(); await publish(); },
  failNext: (command: string) => { failCommand = command; },
  hotkeyMenu: async () => { Object.assign(state, { menuRequestId: ++menuRequestSequence }); await publish(); },
  hotkeyNewRoom: async () => {
    if (state.listening || state.busy) state.error = 'Останови расшифровку и дождись окончания запроса';
    else { create(); Object.assign(state, { focusEntryId: null }); }
    await publish();
  },
  setProviderUnavailable: async () => {
    state.providers[state.settings.provider] = { connected: false, loading: false, authPending: false, catalogSource: null, models: [], selectedModel: null, status: 'Не подключено', budgetConfirmed: false };
    await publish();
  },
  clearAudioSources: async () => {
    state.settings.micEnabled = false; state.settings.systemEnabled = false;
    await publish();
  },
  delaySearch: (query: string, milliseconds: number) => { searchDelays.set(query.toLocaleLowerCase('ru'), milliseconds); },
  delayCommand: (command: string, provider: ProviderId, milliseconds: number) => { commandDelays.set(`${command}:${provider}`, milliseconds); },
  delayAuth: (provider: ProviderId, milliseconds: number) => { authDelays.set(provider, milliseconds); },
  disconnectProviders: async () => { state.providers = structuredClone(emptyState.providers); await publish(); },
  addModel: async (provider: ProviderId, id: string, name: string) => { state.providers[provider].models.push({ id, name }); await publish(); },
  readyProviders: async () => {
    const names = { chatgpt: 'GPT — пример', antigravity: 'Gemini — пример', claude: 'Claude — пример' };
    for (const id of ['chatgpt', 'antigravity', 'claude'] as ProviderId[]) {
      state.providers[id] = { connected: true, loading: false, authPending: false, budgetConfirmed: id !== 'antigravity', status: 'Подключено · тестовые данные', catalogSource: 'Тестовый каталог', selectedModel: `${id}-example`, models: [{ id: `${id}-example`, name: names[id] }, { id: `${id}-example-fast`, name: `${names[id]} · быстро` }] };
    }
    await publish();
  },
  setSummary: async (text: string) => { current().info.summary = text; await publish(); },
  previewWithoutTimings: async () => { current().answer.firstTokenMs = null; current().answer.totalMs = null; await publish(); },
  audioTick: async (microphone: number | null, system: number | null, text?: string) => {
    state.levels = { microphone, system };
    if (text) current().entries.push({ id: `speech-${current().entries.length}`, source: 'system', text, at: new Date().toISOString() });
    await publish();
  },
  seedRecovery: async () => {
    reset();
    Object.assign(state, { roomRecoveries: [{ id: 'damaged-room', primaryError: 'Повреждён тестовый файл комнаты.', backup: { id: 'damaged-room', title: 'Комната из резервной копии', pinned: false, createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), entryCount: 1, summary: '', summaryStale: false }, backupError: null, available: true }] });
    await publish();
  },
  seedLong: async (count = 450) => {
    reset(); const room = create(); room.info.title = 'Длинная тестовая комната';
    room.entries = Array.from({ length: count }, (_, index) => ({ id: `${index + 1}`, source: index % 4 === 0 ? 'assistant' : 'user', text: `Запись ${index + 1}. Контрольный факт: ${index === 4 ? 'АЛЬФА-731' : 'история сохраняется целиком'}.`, at: new Date(1800000000000 + index * 1000).toISOString() }));
    await publish();
  }
} });
ReactDOM.createRoot(document.getElementById('root')!).render(<React.StrictMode><App /></React.StrictMode>);
