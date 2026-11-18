import { useEffect, useRef, useState } from 'react';
import { Icon } from './icons';
import type { Settings } from './types';

export function SettingsDialog({ linux, open, settings, locked, lockedReason, saving, error, onDismissError, onClose, onSave }: {
  linux?: boolean; open: boolean; settings: Settings; locked: boolean; lockedReason: string; saving: boolean;
  error: string | null; onDismissError: () => void; onClose: () => void; onSave: (settings: Settings) => Promise<boolean>;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [draft, setDraft] = useState(settings);
  useEffect(() => {
    if (open) { setDraft(settings); if (!dialog.current?.open) dialog.current?.showModal(); }
    else dialog.current?.close();
    // Draft is copied only when opening, so a background transcript event cannot overwrite edits.
  }, [open]);
  async function save() { if (await onSave(draft)) onClose(); }
  return <dialog ref={dialog} className="settings-dialog" onCancel={onClose} onClose={() => { if (!dialog.current?.open) onClose(); }} aria-labelledby="settings-title">
    <div className="dialog-heading"><h2 id="settings-title">Пути к программам</h2><button className="icon-button" type="button" aria-label="Закрыть настройки" onClick={onClose}><Icon name="close" /></button></div>
    <p className="dialog-intro">Если программа переехала, укажи её новое расположение.</p>
    {error && <div className="error-banner" role="alert"><span>{error}</span><button className="icon-button" type="button" aria-label="Закрыть сообщение об ошибке" onClick={onDismissError}><Icon name="close" size={16} /></button></div>}
    <label className="input-label" htmlFor="antigravity-path">Программа Antigravity</label>
    <input id="antigravity-path" className="path-input" value={draft.antigravityPath} disabled={locked || saving} onChange={event => setDraft({ ...draft, antigravityPath: event.target.value })} placeholder={linux ? 'Путь к agy' : 'Путь к agy.exe'} spellCheck={false} />
    <p className="field-note">While We Talk использует аккаунт, в который ты вошёл в Antigravity.</p>
    {settings.claudePath !== undefined && <><label className="input-label" htmlFor="claude-path">Программа Claude Code</label><input id="claude-path" className="path-input" value={draft.claudePath ?? ''} disabled={locked || saving} onChange={event => setDraft({ ...draft, claudePath: event.target.value })} placeholder={linux ? 'Путь к claude' : 'Путь к claude.exe'} spellCheck={false} /><p className="field-note">Для входа понадобится личная подписка Claude.</p></>}
    {(settings.pythonPath !== undefined || settings.cudaLibPath !== undefined) && <details className="advanced-settings"><summary>Компоненты распознавания</summary>
      {settings.pythonPath !== undefined && <><label className="input-label" htmlFor="python-path">Python для Whisper</label><input id="python-path" className="path-input" value={draft.pythonPath ?? ''} disabled={locked || saving} onChange={event => setDraft({ ...draft, pythonPath: event.target.value })} spellCheck={false} /></>}
      {!linux && settings.cudaLibPath !== undefined && <><label className="input-label" htmlFor="cuda-path">Папка с библиотеками NVIDIA</label><input id="cuda-path" className="path-input" value={draft.cudaLibPath ?? ''} disabled={locked || saving} onChange={event => setDraft({ ...draft, cudaLibPath: event.target.value })} spellCheck={false} /></>}
    </details>}
    {locked && <p className="dialog-warning">{lockedReason}</p>}
    <div className="dialog-actions"><button className="button button-secondary" type="button" onClick={onClose}>Закрыть</button><button className="button button-primary" type="button" disabled={locked || saving} onClick={() => void save()}>{saving ? 'Сохраняю…' : 'Сохранить'}</button></div>
  </dialog>;
}
