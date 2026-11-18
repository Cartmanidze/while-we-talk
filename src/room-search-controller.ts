import type { RoomSearchHit } from './types';
export interface RoomSearchState { query: string; hits: RoomSearchHit[]; loading: boolean; error: string | null }
export type SearchRooms = (query: string) => Promise<RoomSearchHit[]>;
export class RoomSearchController {
  private readonly search: SearchRooms;
  private readonly changed: (state: RoomSearchState) => void;
  private generation = 0;
  private timer: ReturnType<typeof setTimeout> | undefined;
  constructor(search: SearchRooms, changed: (state: RoomSearchState) => void) { this.search = search; this.changed = changed; }
  setQuery(query: string): void {
    this.cancel();
    const current = this.generation;
    const normalized = query.trim();
    if (!normalized || normalized.length > 200) {
      this.changed({ query: normalized, hits: [], loading: false, error: normalized.length > 200 ? 'Сократи запрос до 200 символов.' : null });
      return;
    }
    this.changed({ query: normalized, hits: [], loading: true, error: null });
    this.timer = setTimeout(() => { this.timer = undefined; void this.execute(normalized, current); }, 250);
  }
  private async execute(query: string, generation: number) {
    try {
      const hits = await this.search(query);
      if (generation === this.generation) this.changed({ query, hits, loading: false, error: null });
    } catch (error) {
      if (generation === this.generation) this.changed({ query, hits: [], loading: false, error: error instanceof Error ? error.message : String(error) });
    }
  }
  cancel(): void { this.generation++; if (this.timer !== undefined) clearTimeout(this.timer); this.timer = undefined; }
}
