import { useCallback, useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react';
import { Icon } from './icons';
import { useAppState } from './use-app-state';
import { canManageRooms, canRequestAnswer, canStartListening, canSummarizeRoom, providerAvailable } from './ui-state';
import { ConversationPanel } from './conversation-panel';
import { LiveSources } from './source-panel';
import { AnswerPanel } from './answer-panel';
import { SettingsDialog } from './settings-dialog';
import { ControlsDialog } from './controls-dialog';
import { ConnectionsDialog } from './connections-dialog';
import { RoomsPanel } from './rooms-panel';
import { RoomHeader } from './room-header';
import { SummaryPanel } from './summary-panel';
import { RoomRecoveryPanel } from './room-recovery-panel';
import { RoomDeleteConfirm } from './room-delete-confirm';
import { RoomTrashDialog } from './room-trash-dialog';
import { RoomDeletionNotice } from './room-deletion-notice';
import type { RoomInfo, Settings } from './types';
import { nextRoomTab, type RoomTab } from './room-tabs';
import { commandPending } from './command-pending';
import { providerNames } from './provider-connection';

const roomCommands = ['create_room', 'open_room', 'rename_room', 'pin_room', 'add_message', 'load_more_history', 'recover_room', 'trash_room', 'restore_room'];

export function App() {
  const { state, run, exportRoom, searchRooms, pending, connected, error, dismissError } = useAppState();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [controlsOpen, setControlsOpen] = useState(false);
  const [connectionsOpen, setConnectionsOpen] = useState(false);
  const [trashOpen, setTrashOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<Pick<RoomInfo, 'id' | 'title' | 'entryCount'> | null>(null);
  const [view, setView] = useState<RoomTab>('conversation');
  const menuRequest = useRef({ seen: 0, handledThrough: 0 });
  const focusSequence = useRef(0);
  const [focusRequest, setFocusRequest] = useState<{ token: number; roomId: string; entryId: string | null } | null>(null);
  const focusPending = focusRequest !== null && (focusRequest.roomId !== state.activeRoomId || focusRequest.entryId !== (state.focusEntryId ?? null));
  const listeningPending = pending.has('start_listening') || pending.has('stop_listening');
  const changingSettings = pending.has('update_settings') || commandPending(pending, 'select_model', state.settings.provider);
  const roomPending = roomCommands.some(command => pending.has(command));
  const summaryBusy = state.summaryBusy || pending.has('summarize_room');
  const answerBusy = state.answer.streaming || pending.has('request_answer');
  const canAnswer = connected && canRequestAnswer(state) && !answerBusy && !summaryBusy && !changingSettings && !roomPending;
  const roomLocked = !connected || !canManageRooms(state) || roomPending || listeningPending || summaryBusy || answerBusy;
  const roomBlockedReason = !connected ? 'Подключаюсь к приложению…' : state.listening || listeningPending ? 'Сначала останови расшифровку. Затем можно сменить комнату или сохранить текст.' : state.busy || summaryBusy || answerBusy ? 'Дождись ответа или отмени запрос.' : roomPending ? 'Применяю изменения…' : '';
  const canSummarize = connected && canSummarizeRoom(state) && !summaryBusy && !roomPending && !changingSettings;
  const summaryBlockedReason = roomBlockedReason || (!state.historyTotal ? 'Чтобы подвести итог, добавь в комнату хотя бы одну реплику.' : !canSummarize ? 'Подключи сервис и выбери модель, которая подведёт итог.' : '');
  const provider = state.providers[state.settings.provider];
  const selectedModel = provider.models.find(model => model.id === provider.selectedModel)?.name;
  const needsModelSetup = !providerAvailable(state);
  const hasSource = (state.settings.micEnabled && state.devices.microphones.length > 0)
    || (state.settings.systemEnabled && state.devices.system.length > 0);
  const soundStatus = state.listening ? 'Слушаю' : !state.audioReady ? (state.audioStatus === 'Не удалось запустить распознавание' ? 'Не удалось включить распознавание' : 'Подготовка звука…') : !hasSource ? 'Выбери источник звука' : 'Можно начинать';
  const answerBlockedReason = summaryBusy ? 'Модель ещё подводит итог.' : state.busy || roomPending ? 'Дождись окончания текущего действия.' : 'Для ответа нужна хотя бы одна реплика. Включи расшифровку или добавь текст.';
  const trashRooms = state.trashRooms ?? [];
  const deletingRoom = deleteTarget && !trashRooms.some(room => room.id === deleteTarget.id) ? state.rooms.find(room => room.id === deleteTarget.id) ?? deleteTarget : null;
  const updateSettings = useCallback((settings: Settings) => run('update_settings', { settings }), [run]);
  const requestAnswer = useCallback(() => { if (canAnswer) void run('request_answer'); }, [canAnswer, run]);
  async function createRoom() { if (await run('create_room')) setFocusRequest(null); }
  function openConnections() { setControlsOpen(false); setSettingsOpen(false); setTrashOpen(false); setConnectionsOpen(true); }
  function openComponents() { setControlsOpen(false); setConnectionsOpen(false); setTrashOpen(false); setSettingsOpen(true); }
  async function deleteRoom() {
    if (deletingRoom && !roomLocked && await run('trash_room', { roomId: deletingRoom.id })) setDeleteTarget(null);
  }
  async function restoreRoom(roomId: string) { return !roomLocked && await run('restore_room', { roomId }); }
  async function openRoom(roomId: string, entryId: string | null = null) {
    const previous = focusRequest;
    const request = { token: ++focusSequence.current, roomId, entryId };
    setFocusRequest(request);
    setView('conversation');
    if (!await run('open_room', { roomId, entryId })) setFocusRequest(current => current?.token === request.token ? previous : current);
  }
  async function recoverRoom(roomId: string) {
    const restored = await run('recover_room', { roomId });
    if (restored) setFocusRequest(null);
    return restored;
  }
  function handleTabKey(event: ReactKeyboardEvent<HTMLButtonElement>, current: RoomTab) {
    if (event.ctrlKey || event.altKey || event.metaKey) return;
    const next = nextRoomTab(current, event.key);
    if (next) { event.preventDefault(); setView(next); document.getElementById(`tab-${next}`)?.focus(); }
  }
  useLayoutEffect(() => {
    setView('conversation'); setControlsOpen(false); setSettingsOpen(false); setConnectionsOpen(false); setTrashOpen(false); setDeleteTarget(null); setFocusRequest(null);
  }, [state.activeRoomId]);
  useEffect(() => {
    const id = state.menuRequestId ?? 0;
    if (!Number.isSafeInteger(id) || id < 0) return;
    const previous = menuRequest.current.seen;
    menuRequest.current.seen = id;
    if (id === 0 || id === previous || id <= menuRequest.current.handledThrough) return;
    menuRequest.current.handledThrough = id;
    setSettingsOpen(false); setConnectionsOpen(false); setTrashOpen(false); setDeleteTarget(null); setControlsOpen(true);
  }, [state.menuRequestId]);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target;
      const editable = target instanceof HTMLElement && Boolean(target.closest('input, textarea, [contenteditable="true"]'));
      if (event.ctrlKey && event.key === 'Enter' && !event.repeat && !event.isComposing && !editable && !settingsOpen && !controlsOpen && !connectionsOpen && !trashOpen && !deleteTarget && canAnswer) { event.preventDefault(); requestAnswer(); }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [canAnswer, settingsOpen, controlsOpen, connectionsOpen, trashOpen, deleteTarget, requestAnswer]);

  return <div className="desktop-shell">
    <div className="workspace rooms-workspace">
      <RoomsPanel rooms={state.rooms} trashRooms={trashRooms} deletionNotice={state.deletionNotice ?? null} activeId={state.activeRoomId} locked={roomLocked} blockedReason={roomBlockedReason} creating={pending.has('create_room')} searchRooms={searchRooms} onCreate={() => void createRoom()} onOpen={(roomId, entryId) => void openRoom(roomId, entryId)} onPin={(roomId, pinned) => void run('pin_room', { roomId, pinned })} onDelete={setDeleteTarget} onOpenTrash={() => setTrashOpen(true)} onSettings={() => setControlsOpen(true)} />
      <main className="main-area rooms-main">
        {error && <div className="error-banner" role="alert"><span>{error}</span><button className="icon-button" type="button" aria-label="Закрыть сообщение об ошибке" onClick={dismissError}><Icon name="close" size={17} /></button></div>}
        <RoomDeletionNotice room={trashRooms.find(room => room.id === state.deletionNotice)} locked={roomLocked} restoring={pending.has('restore_room')} onRestore={restoreRoom} />
        {deletingRoom && <RoomDeleteConfirm room={deletingRoom} locked={roomLocked} deleting={pending.has('trash_room')} onDelete={() => void deleteRoom()} onCancel={() => setDeleteTarget(null)} />}
        <RoomRecoveryPanel recoveries={state.roomRecoveries ?? []} notice={state.recoveryNotice ?? null} locked={roomLocked} recovering={pending.has('recover_room')} onRecover={recoverRoom} />
        {!state.activeRoomId ? <div className="no-room-state">
          <div className="wave-print" aria-hidden="true" />
          <h1>{state.rooms.length ? 'Выбери комнату' : 'Создай комнату для разговора'}</h1>
          <p>{state.rooms.length ? 'Открой сохранённую комнату или создай новую.' : 'Создай комнату, чтобы записать разговор или добавить заметку. Расшифровку можно включить позже.'}</p>
          <button className="button button-primary" type="button" disabled={roomLocked} aria-keyshortcuts="Control+Alt+N" title="Создать комнату · Ctrl+Alt+N" onClick={() => void createRoom()}><Icon name="plus" size={18} />{pending.has('create_room') ? 'Создаю…' : 'Создать комнату'}</button>
        </div>
          : <>
            <header className="session-header">
              <RoomHeader key={state.activeRoomId} roomId={state.activeRoomId} title={state.roomTitle} locked={roomLocked} renaming={pending.has('rename_room')} exporting={!connected || pending.has('export_room')} onRename={title => run('rename_room', { title })} onExport={() => exportRoom(state.activeRoomId!)} onDelete={() => setDeleteTarget({ id: state.activeRoomId!, title: state.roomTitle, entryCount: state.historyTotal })} />
              <div className="recording-controls">
                <span className={`listening-status ${state.listening ? 'active' : ''}`} role="status" title={state.audioStatus}><span className={`status-dot ${state.listening ? 'online' : ''}`} />{soundStatus}</span>
                {state.listening && <LiveSources settings={state.settings} levels={state.levels} />}
                <button className={`button listening-button ${state.listening ? 'button-stop' : 'button-primary'}`} type="button" title={!hasSource ? 'Выбери источник в настройках звука' : !state.audioReady ? state.audioStatus : undefined} disabled={!connected || listeningPending || (!state.listening && (!canStartListening(state) || changingSettings || roomPending || answerBusy || summaryBusy))} onClick={() => void run(state.listening ? 'stop_listening' : 'start_listening')}><Icon name={state.listening ? 'stop' : 'mic'} size={17} /><span>{listeningPending ? state.listening ? 'Останавливаю…' : 'Запускаю…' : state.listening ? 'Остановить расшифровку' : 'Начать расшифровку'}</span></button>
              </div>
            </header>
            <div className="room-navigation"><div className="room-tabs" role="tablist" aria-label="Раздел комнаты"><button id="tab-conversation" role="tab" tabIndex={view === 'conversation' ? 0 : -1} aria-selected={view === 'conversation'} aria-controls="panel-conversation" onClick={() => setView('conversation')} onKeyDown={event => handleTabKey(event, 'conversation')} type="button">Разговор</button><button id="tab-summary" role="tab" tabIndex={view === 'summary' ? 0 : -1} aria-selected={view === 'summary'} aria-controls="panel-summary" onClick={() => setView('summary')} onKeyDown={event => handleTabKey(event, 'summary')} type="button">Итог{state.summaryStale && state.roomSummary && <i className="stale-dot" title="Есть новые записи" />}</button></div><button className="current-model text-button" type="button" onClick={() => setControlsOpen(true)} title={selectedModel ? `${providerNames[state.settings.provider]} · ${selectedModel}` : 'Выбрать сервис и модель'}>{selectedModel ? `${providerNames[state.settings.provider]} · ${selectedModel}` : 'Выбрать модель'}<Icon name="chevron" size={13} /></button></div>
            <div className="content-grid room-content-grid">
              <div className="active-room-panel" id="panel-conversation" role="tabpanel" aria-labelledby="tab-conversation" tabIndex={0} hidden={view !== 'conversation'}>
                <ConversationPanel key={state.activeRoomId} roomId={state.activeRoomId} visible={view === 'conversation'} focusEntryId={state.focusEntryId ?? null} focusRequestId={focusRequest?.token ?? 0} focusPending={focusPending} entries={state.transcript} listening={state.listening} total={state.historyTotal} loadingEarlier={pending.has('load_more_history')} adding={pending.has('add_message')} addEnabled={!roomLocked} blockedReason={roomBlockedReason} contextStatus={state.busy ? state.contextStatus : ''} onLoadMore={before => run('load_more_history', { before })} onAddMessage={text => run('add_message', { text })} />
              </div>
              <div className="active-room-panel" id="panel-summary" role="tabpanel" aria-labelledby="tab-summary" tabIndex={0} hidden={view !== 'summary'}>
                <SummaryPanel text={state.roomSummary} stale={state.summaryStale} busy={summaryBusy} enabled={canSummarize} blockedReason={summaryBlockedReason} contextStatus={state.contextStatus} onSummarize={() => void run('summarize_room')} onCancel={() => void run('cancel_answer')} />
              </div>
              <AnswerPanel answer={state.answer} busy={answerBusy && !summaryBusy} enabled={canAnswer} setupRequired={needsModelSetup} setupBusy={provider.loading || state.busy || changingSettings} blockedReason={answerBlockedReason} contextStatus={state.contextStatus} onSetup={openConnections} onAnswer={requestAnswer} onCancel={() => void run('cancel_answer')} />
            </div>
          </>}
      </main>
    </div>
    <ControlsDialog open={controlsOpen} state={state} connected={connected} pending={pending} run={run} changingSettings={changingSettings} error={error} onDismissError={dismissError} onChange={settings => void updateSettings(settings)} onClose={() => setControlsOpen(false)} onComponents={openComponents} onConnections={openConnections} />
    <ConnectionsDialog open={connectionsOpen} state={state} connected={connected} pending={pending} run={run} error={error} onDismissError={dismissError} onChange={settings => void updateSettings(settings)} onClose={() => setConnectionsOpen(false)} onComponents={openComponents} />
    <RoomTrashDialog open={trashOpen} rooms={trashRooms} locked={roomLocked} lockedReason={roomBlockedReason} restoring={pending.has('restore_room')} error={error} onDismissError={dismissError} onRestore={restoreRoom} onClose={() => setTrashOpen(false)} />
    <SettingsDialog linux={state.platform === 'linux'} open={settingsOpen} settings={state.settings} locked={!connected || state.listening || state.busy || state.summaryBusy || changingSettings} lockedReason={!connected ? 'Подожди, пока приложение откроется.' : 'Останови расшифровку и дождись окончания запроса.'} saving={pending.has('update_settings')} error={error} onDismissError={dismissError} onClose={() => setSettingsOpen(false)} onSave={updateSettings} />
  </div>;
}
