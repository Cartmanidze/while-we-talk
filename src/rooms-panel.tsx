import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import { availableSearchHits, filterRooms } from './room-helpers';
import { Icon } from './icons';
import type { RoomInfo, RoomTrashInfo } from './types';
import { MarkdownPreview } from './markdown-content';
import { useRoomSearch } from './use-room-search';
import type { SearchRooms } from './room-search-controller';

function dateLabel(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? '' : date.toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' });
}

export function RoomsPanel({ rooms, trashRooms, deletionNotice, activeId, locked, blockedReason, creating, searchRooms, onCreate, onOpen, onPin, onDelete, onOpenTrash, onSettings }: {
  rooms: RoomInfo[]; activeId: string | null; locked: boolean; blockedReason: string;
  creating: boolean; searchRooms: SearchRooms; onCreate: () => void; onOpen: (roomId: string, entryId?: string | null) => void; onPin: (roomId: string, pinned: boolean) => void;
  trashRooms: RoomTrashInfo[]; deletionNotice: string | null; onDelete: (room: RoomInfo) => void; onOpenTrash: () => void; onSettings: () => void;
}) {
  const [query, setQuery] = useState('');
  const [collapsed, setCollapsed] = useState(false);
  const toggle = useRef<HTMLButtonElement>(null);
  const searchInput = useRef<HTMLInputElement>(null);
  const focusSearch = useRef(false);
  useLayoutEffect(() => {
    if (!collapsed && focusSearch.current) { searchInput.current?.focus(); focusSearch.current = false; }
  }, [collapsed]);
  const search = useRoomSearch(query, searchRooms);
  const searching = Boolean(query.trim());
  const ordered = useMemo(() => filterRooms(rooms, ''), [rooms]);
  const currentRooms = useMemo(() => new Map(rooms.map(room => [room.id, room])), [rooms]);
  const visible = availableSearchHits(searching ? search.hits : ordered.map(room => ({ room, snippet: '', entryId: null })), trashRooms, deletionNotice);
  const toggleLabel = collapsed ? 'Развернуть список комнат' : 'Свернуть список комнат';
  return <aside className={`rooms-sidebar${collapsed ? ' is-collapsed' : ''}`} aria-label="Комнаты">
    <div className="brand"><span className="brand-mark"><Icon name="wave" size={22} /></span><strong>While We Talk</strong></div>
    <div className="rooms-heading"><h2>Комнаты</h2><span>{rooms.length}</span>
      <button className="icon-button rail-toggle" type="button" ref={toggle} aria-label={toggleLabel} aria-expanded={!collapsed} aria-controls="room-browser" title={toggleLabel} onClick={() => setCollapsed(!collapsed)}><Icon name="sidebar" size={18} /></button>
    </div>
    {collapsed && <button className="icon-button" type="button" aria-label="Открыть поиск по комнатам" title="Поиск по комнатам" onClick={() => { focusSearch.current = true; setCollapsed(false); }}><Icon name="search" size={18} /></button>}
    {collapsed ? <button className="icon-button" type="button" aria-label="Открыть список комнат" title="Комнаты" onClick={() => { setCollapsed(false); toggle.current?.focus(); }}><Icon name="folder" size={18} /></button>
      : <button className="button button-secondary new-room-button" type="button" disabled={locked} onClick={onCreate} aria-keyshortcuts="Control+Alt+N" title="Новая комната · Ctrl+Alt+N"><Icon name="plus" size={17} />{creating ? 'Создаю комнату…' : 'Новая комната'}</button>}
    <div id="room-browser" hidden={collapsed}>
    <label className="search-field room-search"><Icon name="search" size={16} /><input ref={searchInput} type="search" maxLength={200} value={query} onChange={event => setQuery(event.target.value)} placeholder="Поиск" aria-label="Поиск по комнатам и всей истории" /></label>
    {searching && search.loading && <p className="room-search-status" role="status">Ищу в комнатах и сообщениях…</p>}
    {searching && search.error && <div className="room-search-error" role="alert"><p>{search.error}</p><button className="text-button" type="button" onClick={search.retry}>Повторить поиск</button></div>}
    {searching && !search.loading && !search.error && <p className="room-search-status" role="status">Найдено: {visible.length}</p>}
    {locked && blockedReason && <p className="room-lock-note" role="status">{blockedReason}</p>}
    <div className="rooms-list" aria-label="Сохранённые комнаты">
      {!visible.length && !search.loading && !search.error ? <div className="rooms-empty"><p>{searching ? 'Ничего не нашлось. Попробуй другое слово.' : 'Пока нет сохранённых комнат'}</p></div>
        : visible.map((hit, index) => { const room = currentRooms.get(hit.room.id) ?? hit.room; return <div key={`${room.id}:${hit.entryId ?? 'room'}:${index}`} className={`room-item ${activeId === room.id ? 'active' : ''} ${room.pinned ? 'pinned' : ''}`}>
          <button className="room-open" type="button" disabled={locked} onClick={() => onOpen(room.id, hit.entryId)} aria-current={activeId === room.id ? 'page' : undefined} aria-label={`Открыть комнату «${room.title}»`}>
            <strong title={room.title}>{room.title}</strong>
            {searching && hit.snippet && <MarkdownPreview text={hit.snippet} />}
            {searching && hit.entryId && <span className="search-hit-location">Совпадение в истории</span>}
            <span className="room-meta"><time dateTime={room.updatedAt}>{dateLabel(room.updatedAt)}</time>{room.summaryStale && <span className="stale-dot" title="После последнего итога появились новые реплики" />}</span>
          </button>
          <button className="room-pin icon-button" type="button" disabled={locked} onClick={() => onPin(room.id, !room.pinned)} aria-label={`${room.pinned ? 'Открепить' : 'Закрепить'} комнату «${room.title}»`} aria-pressed={room.pinned} title={room.pinned ? 'Открепить' : 'Закрепить'}><Icon name="pin" size={14} /></button>
          <button className="room-trash icon-button" type="button" disabled={locked} onClick={() => onDelete(room)} aria-label={`Удалить комнату «${room.title}»`} title="Переместить в корзину"><Icon name="trash" size={14} /></button>
        </div>; })}
    </div>
    </div>
    <div className="rooms-local-note">
      <button className="text-button" type="button" onClick={onOpenTrash} aria-label={collapsed ? 'Корзина' : undefined} title="Корзина"><Icon name="trash" size={16} /><span className="rail-caption">Корзина</span>{trashRooms.length > 0 && <span>{trashRooms.length}</span>}</button>
      <button className="text-button" type="button" onClick={onSettings} aria-label="Настройки" aria-keyshortcuts="Control+Alt+M" title="Настройки · Ctrl+Alt+M"><Icon name="settings" size={16} /><span className="rail-caption">Настройки</span></button>
      <span><Icon name="folder" size={13} />Комнаты хранятся на этом компьютере</span>
    </div>
  </aside>;
}
