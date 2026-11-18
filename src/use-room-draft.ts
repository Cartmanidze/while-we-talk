import { useCallback, useSyncExternalStore } from 'react';
import { RoomDraftStore } from './room-draft-store';

const drafts = new RoomDraftStore(() => typeof window === 'undefined' ? null : window.sessionStorage);

export function useRoomDraft(roomId: string) {
  const subscribe = useCallback((listener: () => void) => drafts.subscribe(roomId, listener), [roomId]);
  const snapshot = useCallback(() => drafts.read(roomId), [roomId]);
  const draft = useSyncExternalStore(subscribe, snapshot, snapshot);
  return {
    ...draft,
    setText: useCallback((text: string) => drafts.write(roomId, text), [roomId]),
    clear: useCallback(() => drafts.clear(roomId), [roomId]),
    clearIfUnchanged: useCallback((sentText: string) => drafts.clearIfUnchanged(roomId, sentText), [roomId]),
  };
}
