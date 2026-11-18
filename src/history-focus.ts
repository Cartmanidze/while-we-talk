export class HistoryFocus {
  private handled: string | null = null;
  next(roomId: string, entryId: string | null, requestId: number, visible: boolean, pending: boolean, availableIds: string[]): string | null {
    if (!visible || pending) return null;
    if (entryId === null) { this.handled = null; return null; }
    const key = JSON.stringify([roomId, entryId, requestId]);
    if (this.handled === key || !availableIds.includes(entryId)) return null;
    this.handled = key;
    return entryId;
  }
}
