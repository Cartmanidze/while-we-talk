export type RoomTab = 'conversation' | 'summary';
export function nextRoomTab(current: RoomTab, key: string): RoomTab | null {
  if (key === 'Home') return 'conversation';
  if (key === 'End') return 'summary';
  if (key === 'ArrowLeft' || key === 'ArrowRight') return current === 'conversation' ? 'summary' : 'conversation';
  return null;
}
