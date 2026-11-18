import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { emptyState, type AppState, type RoomSearchHit } from './types';
import { commandKey } from './command-pending';

export function useAppState() {
  const [state, setState] = useState<AppState>(emptyState);
  const [pending, setPending] = useState<Set<string>>(new Set());
  const [connectionError, setConnectionError] = useState<string | null>(null);
  const [connected, setConnected] = useState(false);
  const pendingRef = useRef(new Set<string>());

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let eventsReceived = 0;
    async function connect() {
      if (!isTauri()) {
        setConnectionError('Открой While We Talk на компьютере, чтобы включить звук и выбрать модель.');
        return;
      }
      try {
        unlisten = await listen<AppState>('app-state', event => {
          eventsReceived++;
          if (!disposed) { setState(event.payload); setConnected(true); }
        });
        if (disposed) { unlisten(); return; }
        const before = eventsReceived;
        const initial = await invoke<AppState>('get_state');
        if (!disposed) {
          if (eventsReceived === before) setState(initial);
          setConnected(true);
          setConnectionError(null);
        }
      } catch (error) {
        if (!disposed) setConnectionError(String(error));
      }
    }
    void connect();
    return () => { disposed = true; unlisten?.(); };
  }, []);

  const run = useCallback(async (command: string, args?: Record<string, unknown>): Promise<boolean> => {
    const key = commandKey(command, args);
    if (!isTauri() || pendingRef.current.has(key)) return false;
    pendingRef.current.add(key);
    setPending(new Set(pendingRef.current));
    setConnectionError(null);
    try {
      await invoke(command, args);
      return true;
    } catch (error) {
      setConnectionError(String(error));
      return false;
    } finally {
      pendingRef.current.delete(key);
      setPending(new Set(pendingRef.current));
    }
  }, []);

  const dismissError = useCallback(() => {
    setConnectionError(null);
    if (state.error) void run('dismiss_error');
  }, [run, state.error]);
  const exportRoom = useCallback(async (roomId: string): Promise<string | null> => {
    const command = 'export_room';
    if (!isTauri() || pendingRef.current.has(command)) return null;
    pendingRef.current.add(command);
    setPending(new Set(pendingRef.current));
    setConnectionError(null);
    try { return await invoke<string>(command, { roomId }); }
    catch (error) { setConnectionError(String(error)); return null; }
    finally { pendingRef.current.delete(command); setPending(new Set(pendingRef.current)); }
  }, []);
  const searchRooms = useCallback(async (query: string): Promise<RoomSearchHit[]> => {
    if (!isTauri()) throw new Error('Поиск по истории доступен в приложении While We Talk.');
    return invoke<RoomSearchHit[]>('search_rooms', { query });
  }, []);
  return { state, run, exportRoom, searchRooms, pending, connected, error: connectionError || state.error, dismissError };
}
