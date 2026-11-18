import { useEffect, useState } from 'react';
import { Icon } from './icons';
import type { RoomTrashInfo } from './types';

export function RoomDeletionNotice({ room, locked, restoring, onRestore }: {
  room: RoomTrashInfo | undefined; locked: boolean; restoring: boolean; onRestore: (roomId: string) => Promise<boolean>;
}) {
  const identity = room ? `${room.id}:${room.deletedAt}` : '';
  const [dismissed, setDismissed] = useState('');
  const [restored, setRestored] = useState('');
  useEffect(() => { if (identity) setRestored(''); }, [identity]);
  async function restore() {
    if (room && await onRestore(room.id)) setRestored(`«${room.title}» снова в списке комнат.`);
  }
  if (restored) return <div className="room-deletion-notice" role="status"><span>{restored}</span><button className="icon-button" type="button" aria-label="Закрыть сообщение о восстановлении" onClick={() => setRestored('')}><Icon name="close" size={15} /></button></div>;
  if (!room || dismissed === identity) return null;
  return <div className="room-deletion-notice" role="status"><span>«{room.title}» в корзине.</span><button className="text-button" type="button" disabled={locked || restoring} onClick={() => void restore()}>Вернуть</button><button className="icon-button" type="button" aria-label="Закрыть сообщение об удалении" onClick={() => setDismissed(identity)}><Icon name="close" size={15} /></button></div>;
}
