import { useLayoutEffect, useRef, useState } from 'react';
import { Icon } from './icons';
import type { RoomTrashInfo } from './types';

function deletedAt(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString('ru-RU', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
}
export function RoomTrashDialog({ open, rooms, locked, lockedReason, restoring, error, onDismissError, onRestore, onClose }: {
  open: boolean; rooms: RoomTrashInfo[]; locked: boolean; lockedReason: string; restoring: boolean; error: string | null;
  onDismissError: () => void; onRestore: (roomId: string) => Promise<boolean>; onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [notice, setNotice] = useState('');
  useLayoutEffect(() => { if (open) { setNotice(''); if (!dialog.current?.open) dialog.current?.showModal(); } else dialog.current?.close(); }, [open]);
  async function restore(room: RoomTrashInfo) { if (await onRestore(room.id)) setNotice(`«${room.title}» снова в списке комнат.`); }
  return <dialog className="settings-dialog room-trash-dialog" ref={dialog} aria-labelledby="trash-title" onCancel={onClose} onClose={() => { if (!dialog.current?.open) onClose(); }}>
    <div className="dialog-heading"><h2 id="trash-title">Корзина</h2><button className="icon-button" type="button" aria-label="Закрыть корзину" onClick={onClose}><Icon name="close" /></button></div>
    <p className="dialog-intro">Восстанови комнату, чтобы снова открыть её историю и итог.</p>
    {error && <div className="error-banner" role="alert"><span>{error}</span><button className="icon-button" type="button" aria-label="Закрыть сообщение об ошибке" onClick={onDismissError}><Icon name="close" size={16} /></button></div>}
    {notice && <p className="trash-restored-notice" role="status">{notice}</p>}
    {rooms.length ? <div className="trash-room-list">{rooms.map(room => <article className="trash-room" key={room.id}><div><h3>{room.title}</h3><p><time dateTime={room.deletedAt}>{deletedAt(room.deletedAt)}</time> · {room.entryCount === null ? 'Не удалось посчитать записи' : `Записей: ${room.entryCount}`}</p></div><button className="button button-secondary" type="button" disabled={locked || restoring} onClick={() => void restore(room)}>Восстановить</button></article>)}</div> : <p className="trash-empty">Корзина пуста</p>}
    {locked && lockedReason && <p className="field-note">{lockedReason}</p>}
  </dialog>;
}
