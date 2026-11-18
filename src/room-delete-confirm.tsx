import type { RoomInfo } from './types';

export function RoomDeleteConfirm({ room, locked, deleting, onDelete, onCancel }: {
  room: Pick<RoomInfo, 'id' | 'title' | 'entryCount'>; locked: boolean; deleting: boolean; onDelete: () => void; onCancel: () => void;
}) {
  return <section className="room-delete-confirm" aria-label="Удаление комнаты" onKeyDown={event => { if (event.key === 'Escape') { event.preventDefault(); onCancel(); } }}>
    <div><h3>Переместить «{room.title}» в корзину?</h3><p>Комнату можно вернуть вместе с историей и итогом. Записей в комнате: {room.entryCount}.</p></div>
    <div className="room-delete-actions"><button autoFocus className="button button-primary" type="button" disabled={locked || deleting} onClick={onDelete}>{deleting ? 'Перемещаю…' : 'Переместить в корзину'}</button><button className="text-button" type="button" disabled={deleting} onClick={onCancel}>Отмена</button></div>
  </section>;
}
