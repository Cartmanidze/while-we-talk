import { useLayoutEffect, useRef } from 'react';
import { Icon } from './icons';
import { SourcePanel } from './source-panel';
import { ProviderPanel } from './provider-panel';
import { WindowAppearancePanel } from './window-appearance-panel';
import type { AppState, RunCommand, Settings } from './types';

export function ControlsDialog({ open, state, connected, pending, run, changingSettings, error, onDismissError, onChange, onClose, onComponents, onConnections }: {
  open: boolean; state: AppState; connected: boolean; pending: Set<string>; run: RunCommand; changingSettings: boolean;
  error: string | null; onDismissError: () => void; onChange: (settings: Settings) => void; onClose: () => void; onComponents: () => void; onConnections: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  useLayoutEffect(() => { if (open) { if (!dialog.current?.open) dialog.current?.showModal(); } else dialog.current?.close(); }, [open, state.menuRequestId]);
  return <dialog className="settings-dialog controls-dialog" ref={dialog} onCancel={onClose} onClose={() => { if (!dialog.current?.open) onClose(); }} aria-labelledby="controls-title">
    <div className="dialog-heading"><h2 id="controls-title">Настройки</h2><button className="icon-button" type="button" aria-label="Закрыть настройки" onClick={onClose}><Icon name="close" /></button></div>
    {error && <div className="error-banner" role="alert"><span>{error}</span><button className="icon-button" type="button" aria-label="Закрыть сообщение об ошибке" onClick={onDismissError}><Icon name="close" size={16} /></button></div>}
    <div className="controls-dialog-grid"><SourcePanel state={state} locked={!connected || state.listening || state.busy || state.summaryBusy || changingSettings || pending.has('start_listening') || pending.has('stop_listening')} change={onChange} /><ProviderPanel state={state} run={run} pending={pending} connected={connected} change={onChange} onConnections={onConnections} /></div>
    {state.platform === 'linux'
      ? <p className="field-note">В Linux окно видно при демонстрации всего экрана. Для звонка выбери нужное окно в программе демонстрации.</p>
      : <WindowAppearancePanel settings={state.settings} applied={state.windowAppearanceApplied} connected={connected} run={run} />}
    <div className="hotkeys-help" role="group" aria-label="Горячие клавиши">
      <div className="hotkeys-list"><span><kbd>Ctrl+Alt+M</kbd> Настройки</span><span><kbd>Ctrl+Alt+N</kbd> Новая комната</span></div>
      <p>Работают, пока приложение запущено.</p>
      {state.hotkeyError && <p className="hotkey-warning" role="alert">{state.hotkeyError}</p>}
    </div>
    <div className="controls-dialog-footer"><p>Речь расшифровывается на компьютере. Модель получает текст, когда ты просишь ответ или итог.</p><button className="text-button" type="button" onClick={onComponents}>Пути к программам <Icon name="arrow" size={14} /></button></div>
  </dialog>;
}
