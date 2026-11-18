import type { HistorySource, RoomInfo, RoomSearchHit, RoomTrashInfo } from './types';

export const MAX_MESSAGE_BYTES = 1_000_000;
export function messageBytes(text: string): number { return new TextEncoder().encode(text).length; }

export function filterRooms(rooms: RoomInfo[], query: string): RoomInfo[] {
  const words = query.trim().toLocaleLowerCase('ru').split(/\s+/).filter(Boolean);
  return rooms.filter(room => {
    if (!words.length) return true;
    const text = `${room.title} ${room.summary}`.toLocaleLowerCase('ru');
    return words.every(word => text.includes(word));
  }).sort((a, b) => Number(b.pinned) - Number(a.pinned) || (Date.parse(b.updatedAt) || 0) - (Date.parse(a.updatedAt) || 0));
}
export function sourceLabel(source: HistorySource): string {
  return { microphone: 'Микрофон', system: 'Системный звук', user: 'Текст', assistant: 'Ответ модели' }[source];
}
export function historyWindow(total: number, loaded: number) {
  const remaining = Math.max(0, total - loaded);
  return { remaining, nextPage: Math.min(200, remaining) };
}

export function availableSearchHits(hits: RoomSearchHit[], trash: RoomTrashInfo[], deletionNotice: string | null): RoomSearchHit[] {
  const hidden = new Set(trash.map(room => room.id));
  if (deletionNotice) hidden.add(deletionNotice);
  return hits.filter(hit => !hidden.has(hit.room.id));
}
