export interface DraftSnapshot { readonly text: string; readonly persisted: boolean }
export type DraftStorage = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;

export class RoomDraftStore {
  private readonly storage: () => DraftStorage | null;
  private readonly memory = new Map<string, DraftSnapshot>();
  private readonly listeners = new Map<string, Set<() => void>>();

  constructor(storage: () => DraftStorage | null) { this.storage = storage; }
  private key(roomId: string) { return `sobesednik:draft:v1:${encodeURIComponent(roomId)}`; }

  read(roomId: string): DraftSnapshot {
    const cached = this.memory.get(roomId);
    if (cached) return cached;
    let snapshot: DraftSnapshot = { text: '', persisted: false };
    try {
      const storage = this.storage();
      if (storage) snapshot = { text: storage.getItem(this.key(roomId)) ?? '', persisted: true };
    } catch { /* The draft remains available in memory when browser storage is denied. */ }
    this.memory.set(roomId, snapshot);
    return snapshot;
  }

  write(roomId: string, text: string): void {
    let persisted = false;
    try {
      const storage = this.storage();
      if (storage) {
        if (text) storage.setItem(this.key(roomId), text); else storage.removeItem(this.key(roomId));
        persisted = true;
      }
    } catch { /* Quota/security errors must not discard the unsaved text. */ }
    this.memory.set(roomId, { text, persisted });
    for (const listener of this.listeners.get(roomId) ?? []) listener();
  }

  clear(roomId: string): void { this.write(roomId, ''); }

  clearIfUnchanged(roomId: string, sentText: string): boolean {
    if (this.read(roomId).text !== sentText) return false;
    this.clear(roomId);
    return true;
  }

  subscribe(roomId: string, listener: () => void): () => void {
    const listeners = this.listeners.get(roomId) ?? new Set();
    listeners.add(listener);
    this.listeners.set(roomId, listeners);
    return () => { listeners.delete(listener); if (!listeners.size) this.listeners.delete(roomId); };
  }
}
