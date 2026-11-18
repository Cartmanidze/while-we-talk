import { Icon } from './icons';
import { ModelPicker } from './model-picker';
import { connectionPhase, providerIds, providerNames } from './provider-connection';
import type { AppState, ProviderId, RunCommand, Settings } from './types';
import { commandPending } from './command-pending';

export function ProviderPanel({ state, run, pending, connected, change, onConnections }: {
  state: AppState; run: RunCommand; pending: Set<string>; connected: boolean;
  change: (settings: Settings) => void; onConnections: () => void;
}) {
  const providerId = state.settings.provider;
  const provider = state.providers[providerId];
  const locked = !connected || state.busy || state.summaryBusy || pending.has('update_settings') || pending.has('request_answer') || pending.has('summarize_room');
  const modelLocked = locked || provider.loading || provider.authPending || commandPending(pending, 'select_model', providerId);
  const phase = connectionPhase(providerId, provider);
  return <section className="sidebar-section model-section" aria-labelledby="model-title">
    <div className="section-label" id="model-title"><span>Кто ответит</span></div>
    <label className="model-picker-label" htmlFor="provider-select">Сервис</label>
    <div className="select-wrap"><select id="provider-select" value={providerId} disabled={locked} onChange={event => change({ ...state.settings, provider: event.target.value as ProviderId })}>{providerIds.map(id => <option value={id} key={id}>{providerNames[id]}</option>)}</select><Icon name="chevron" size={15} /></div>
    <div className="compact-model-row"><ModelPicker id="model-select" label="Модель для ответа" providerId={providerId} provider={provider} locked={modelLocked} run={run} /><button type="button" className="icon-button" aria-label="Обновить модели" title="Обновить модели" disabled={locked || provider.loading || commandPending(pending, 'refresh_models', providerId) || (providerId !== 'antigravity' && !provider.connected) || (providerId === 'chatgpt' && !provider.budgetConfirmed)} onClick={() => void run('refresh_models', { provider: providerId })}><Icon name="refresh" size={16} /></button></div>
    <div className="compact-connection-state"><span className={`status-dot ${phase.key === 'ready' ? 'online' : ''}`} /><span>{phase.label}</span></div>
    <button className="button button-secondary connections-open" type="button" onClick={onConnections}>Подключения <Icon name="arrow" size={15} /></button>
  </section>;
}
