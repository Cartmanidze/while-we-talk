import { useCallback, useEffect, useMemo, useState } from 'react';
import { RoomSearchController, type RoomSearchState, type SearchRooms } from './room-search-controller';

const empty: RoomSearchState = { query: '', hits: [], loading: false, error: null };
export function useRoomSearch(query: string, searchRooms: SearchRooms) {
  const [result, setResult] = useState<RoomSearchState>(empty);
  const controller = useMemo(() => new RoomSearchController(searchRooms, setResult), [searchRooms]);
  useEffect(() => { controller.setQuery(query); return () => controller.cancel(); }, [controller, query]);
  const normalized = query.trim();
  const current = result.query === normalized ? result : { query: normalized, hits: [], loading: Boolean(normalized), error: null };
  return { ...current, retry: useCallback(() => controller.setQuery(query), [controller, query]) };
}
