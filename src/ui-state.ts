import type { AppState, ProviderId, ProviderState } from './types';

export function providerReady(id: ProviderId, provider: ProviderState): boolean {
  return !provider.loading && !provider.authPending
    && provider.models.some(model => model.id === provider.selectedModel)
    && (id === 'antigravity' || (provider.connected && provider.budgetConfirmed));
}

export function providerAvailable(state: AppState, id: ProviderId = state.settings.provider): boolean {
  return providerReady(id, state.providers[id]);
}
export function canManageRooms(state: AppState): boolean {
  return !state.busy && !state.summaryBusy && !state.listening;
}
export function canSummarizeRoom(state: AppState): boolean {
  return Boolean(state.activeRoomId) && state.historyTotal > 0 && canManageRooms(state) && providerAvailable(state);
}
export function canRequestAnswer(state: AppState): boolean {
  return Boolean(state.activeRoomId) && !state.busy && !state.summaryBusy && providerAvailable(state)
    && state.transcript.some(entry => entry.source !== 'assistant' && entry.text.trim());
}
export function canStartListening(state: AppState): boolean {
  return Boolean(state.activeRoomId) && state.audioReady && !state.listening && !state.busy && !state.summaryBusy
    && ((state.settings.micEnabled && state.devices.microphones.length > 0)
      || (state.settings.systemEnabled && state.devices.system.length > 0));
}
