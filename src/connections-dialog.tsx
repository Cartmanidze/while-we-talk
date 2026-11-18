import { useLayoutEffect, useRef } from 'react';
import { Icon } from './icons';
import { ConnectionCard } from './connection-card';
import { providerIds } from './provider-connection';
import type { AppState, RunCommand, Settings } from './types';

export function ConnectionsDialog({ open, state, connected, pending, run, error, onDismissError, onChange, onClose, onComponents }: {
  open: boolean; state: AppState; connected: boolean; pending: Set<string>; run: RunCommand; error: string | null;
  onDismissError: () => void; onChange: (settings: Settings) => void; onClose: () => void; onComponents: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  useLayoutEffect(() => { if (open) { if (!dialog.current?.open) dialog.current?.showModal(); } else dialog.current?.close(); }, [open]);
  return <dialog ref={dialog} className="settings-dialog connections-dialog" aria-labelledby="connections-title" onCancel={onClose} onClose={() => { if (!dialog.current?.open) onClose(); }}>
    <div className="dialog-heading"><div><h2 id="connections-title">Подключения</h2><p className="connections-intro">Можно подключить все три сервиса. На каждый запрос отвечает одна выбранная модель.</p></div><button className="icon-button" type="button" aria-label="Закрыть подключения" onClick={onClose}><Icon name="close" /></button></div>
    {error && <div className="error-banner" role="alert"><span>{error}</span><button className="icon-button" type="button" aria-label="Закрыть сообщение об ошибке" onClick={onDismissError}><Icon name="close" size={16} /></button></div>}
    <div className="connections-grid">{providerIds.map(id => <ConnectionCard key={id} id={id} state={state} connected={connected} pending={pending} run={run} onChange={onChange} />)}</div>
    <div className="connections-footer"><p>Вход проходит на сайте сервиса или в его приложении.</p><button className="text-button" type="button" onClick={onComponents}>Пути к программам <Icon name="arrow" size={14} /></button></div>
  </dialog>;
}
