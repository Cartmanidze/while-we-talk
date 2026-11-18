import { useEffect, useState } from 'react';
import { Icon } from './icons';

export function RoomHeader({ roomId, title, locked, renaming, exporting, onRename, onExport, onDelete }: {
  roomId: string; title: string; locked: boolean; renaming: boolean; exporting: boolean;
  onRename: (title: string) => Promise<boolean>; onExport: () => Promise<string | null>; onDelete: () => void;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(title);
  const [exportStatus, setExportStatus] = useState('');
  useEffect(() => { setEditing(false); setDraft(title); setExportStatus(''); }, [roomId]);
  useEffect(() => { if (!editing) setDraft(title); }, [title, editing]);
  async function rename(event: React.FormEvent) {
    event.preventDefault();
    if (!locked && draft.trim() && await onRename(draft.trim())) setEditing(false);
  }
  async function exportMarkdown() {
    const markdown = await onExport();
    if (markdown === null) return;
    try { await navigator.clipboard.writeText(markdown); setExportStatus('Разговор скопирован'); }
    catch { setExportStatus('Не удалось скопировать разговор. Попробуй ещё раз.'); }
  }
  return <div className="room-toolbar">
    <div className="room-title-area">
      {editing ? <form className="room-rename" onSubmit={event => void rename(event)}><input autoFocus aria-label="Название комнаты" value={draft} maxLength={120} disabled={locked || renaming} onChange={event => setDraft(event.target.value)} onFocus={event => event.currentTarget.select()} onKeyDown={event => { if (event.key === 'Escape') setEditing(false); }} /><button className="icon-button" type="submit" disabled={locked || renaming || !draft.trim()} aria-label="Сохранить название"><Icon name="check" size={17} /></button><button className="icon-button" type="button" aria-label="Отменить переименование" onClick={() => setEditing(false)}><Icon name="close" size={16} /></button></form>
        : <div className="room-title-line"><h1 title={title}>{title || 'Новая комната'}</h1><button className="icon-button" type="button" disabled={locked} onClick={() => setEditing(true)} aria-label="Переименовать комнату" title="Переименовать"><Icon name="edit" size={16} /></button></div>}
    </div>
    <div className="room-actions"><div className="export-control"><button className="icon-button" type="button" disabled={exporting} onClick={() => void exportMarkdown()} aria-label="Скопировать разговор в Markdown" title="Скопировать разговор в Markdown"><Icon name="copy" size={16} /></button>{exportStatus && <span className="export-status" role="status">{exportStatus}</span>}</div><button className="icon-button room-delete-button" type="button" disabled={locked} onClick={onDelete} aria-label="Удалить комнату" title="Переместить комнату в корзину"><Icon name="trash" size={16} /></button></div>
  </div>;
}
