import { Icon } from './icons';
import { ModelPicker } from './model-picker';
import { connectionPhase, providerNames } from './provider-connection';
import { providerReady } from './ui-state';
import type { AppState, ProviderId, RunCommand, Settings } from './types';
import { commandPending } from './command-pending';

const descriptions: Record<ProviderId, string> = {
  chatgpt: 'Войди в ChatGPT в браузере, затем вернись сюда.',
  antigravity: 'Войди в Antigravity Desktop, вернись сюда и нажми «Проверить вход».',
  claude: 'Claude Code откроет страницу входа в браузере.',
};

export function ConnectionCard({ id, state, connected, pending, run, onChange }: {
  id: ProviderId; state: AppState; connected: boolean; pending: Set<string>; run: RunCommand; onChange: (settings: Settings) => void;
}) {
  const provider = state.providers[id];
  const name = providerNames[id];
  const phase = connectionPhase(id, provider);
  const selected = state.settings.provider === id;
  const locked = !connected || state.busy || state.summaryBusy;
  const loginPending = id === 'chatgpt' ? pending.has('connect_chatgpt') : commandPending(pending, 'connect_provider', id);
  const checking = provider.loading;
  const modelLocked = locked || checking || provider.authPending || commandPending(pending, 'select_model', id);
  const canCheck = !locked && !checking && !commandPending(pending, 'refresh_models', id) && (id === 'antigravity' || !provider.authPending)
    && (id !== 'chatgpt' || (provider.connected && provider.budgetConfirmed));
  const ready = providerReady(id, provider);
  function connect() { void run(id === 'chatgpt' ? 'connect_chatgpt' : 'connect_provider', id === 'chatgpt' ? undefined : { provider: id }); }
  return <section className={`connection-card ${selected ? 'selected' : ''}`} aria-labelledby={`connection-${id}`}>
    <div className="connection-card-heading"><h3 id={`connection-${id}`}>{name}</h3><span className={`connection-badge ${phase.key === 'ready' ? 'ready' : ''}`}>{phase.label}</span></div>
    <ol className="connection-steps" aria-label={`Подключение ${name}`}>
      {['Вход', id === 'antigravity' ? 'Проверка' : 'Подписка', 'Модель'].map((label, index) => <li key={label} aria-current={phase.step === index + 1 && phase.key !== 'ready' ? 'step' : undefined}><span>{index + 1}</span>{label}</li>)}
    </ol>
    <p className="connection-instructions">{descriptions[id]}</p>
    <div className="connection-actions">
      {provider.authPending ? <button className="button button-secondary" type="button" disabled={!connected || commandPending(pending, 'cancel_provider_login', id)} onClick={() => void run('cancel_provider_login', { provider: id })}>Отменить вход</button>
        : <button className="button button-secondary" type="button" disabled={locked || provider.loading || loginPending} onClick={connect}>{provider.connected ? 'Войти заново' : id === 'antigravity' ? 'Открыть Antigravity' : `Войти через ${name}`}</button>}
      <button className="text-button" type="button" disabled={!canCheck} onClick={() => void run('refresh_models', { provider: id })}><Icon name="refresh" size={14} />Проверить вход</button>
    </div>
    <p className="connection-detail" role="status">{provider.status || 'Вход ещё не проверен.'}</p>
    {id === 'chatgpt' && <div className="connection-budget">
      {provider.budgetConfirmed ? <p className="confirmed-limit"><Icon name="shield" size={15} />Ты подтвердил лимит 30% и отключение платных кредитов.</p>
        : <><p>В настройках ChatGPT выбери это приложение, установи лимит 30% и отключи платные кредиты.</p><label className="budget-confirmation"><input type="checkbox" checked={false} disabled={locked || !provider.connected || provider.loading || provider.authPending || pending.has('confirm_budget')} onChange={event => { if (event.target.checked) void run('confirm_budget', { percent: 30 }); }} /><span>Лимит приложения 30%, платные кредиты выключены</span></label></>}
      <button className="text-button" type="button" onClick={() => void run('open_provider_usage', { provider: id })}>Открыть лимиты ChatGPT ↗</button>
      <p className="connection-small">В ChatGPT приложение называется Local Speech Subscription Check.</p>
    </div>}
    {id === 'claude' && <div className="connection-budget">
      {provider.budgetConfirmed ? <p className="confirmed-limit"><Icon name="shield" size={15} />Ты подтвердил, что Extra usage выключено.</p>
        : <><p>Открой настройки Claude и выключи Extra usage, чтобы не платить сверх подписки.</p><label className="budget-confirmation"><input type="checkbox" checked={false} disabled={locked || !provider.connected || provider.loading || provider.authPending || pending.has('confirm_claude_subscription')} onChange={event => { if (event.target.checked) void run('confirm_claude_subscription'); }} /><span>Extra usage выключено в моём аккаунте Claude</span></label></>}
      <button className="text-button" type="button" onClick={() => void run('open_provider_usage', { provider: id })}>Открыть лимиты Claude ↗</button>
    </div>}
    {id === 'antigravity' && <p className="connection-budget-note">While We Talk запрашивает ответы без дополнительных кредитов Antigravity.</p>}
    <div className="connection-model-area"><ModelPicker id={`connection-model-${id}`} label={`Модель ${name}`} providerId={id} provider={provider} locked={modelLocked} run={run} />
      <p className="connection-small">{provider.models.length ? `Моделей в каталоге: ${provider.models.length}` : 'Список моделей появится после проверки входа.'}{provider.catalogSource ? ` · ${provider.catalogSource}` : ''}</p>
      <button className={`button ${selected ? 'button-secondary' : 'button-primary'}`} type="button" aria-pressed={selected} disabled={locked || !ready || selected || pending.has('update_settings')} onClick={() => onChange({ ...state.settings, provider: id })}>{selected ? 'Выбран для ответов' : 'Выбрать для ответов'}</button>
    </div>
  </section>;
}
