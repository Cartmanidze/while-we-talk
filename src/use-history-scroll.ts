import { useLayoutEffect, useRef, useState } from 'react';
import { HistoryFocus } from './history-focus';
import type { TranscriptEntry } from './types';

interface HistoryView {
  roomId: string; entries: TranscriptEntry[]; total: number; visible: boolean;
  focusEntryId: string | null; focusRequestId: number; focusPending: boolean;
}

export function useHistoryScroll({ roomId, entries, total, visible, focusEntryId, focusRequestId, focusPending }: HistoryView) {
  const scroll = useRef<HTMLDivElement>(null);
  const focus = useRef(new HistoryFocus());
  const anchor = useRef<{ height: number; top: number; firstId: string } | null>(null);
  // The backend sends a moving window. Keep the page under the reader even when it falls outside that window.
  const [snapshot, setSnapshot] = useState<{ entries: TranscriptEntry[]; total: number; request: number } | null>(null);
  const reading = snapshot?.request === focusRequestId && snapshot.total <= total ? snapshot : null;
  const shown = reading?.entries ?? entries;
  const shownTotal = reading?.total ?? total;
  const firstIndex = shownTotal - shown.length;
  const tailId = entries.at(-1)?.id;

  useLayoutEffect(() => {
    const element = scroll.current;
    if (element && visible && !focusPending && !reading) element.scrollTop = element.scrollHeight;
  }, [visible, focusPending, reading, tailId, focusRequestId]);

  useLayoutEffect(() => {
    const element = scroll.current;
    if (!element || !visible || focusPending) return;
    const id = focus.current.next(roomId, focusEntryId, focusRequestId, visible, false, focusEntryId ? entries.map(entry => entry.id) : []);
    if (id) {
      const target = element.querySelector<HTMLElement>(`[data-entry-id="${CSS.escape(id)}"]`);
      if (target) {
        anchor.current = null;
        setSnapshot({ entries, total, request: focusRequestId });
        const reply = target.querySelector('details');
        if (reply) reply.open = true;
        target.scrollIntoView({ block: 'center', inline: 'nearest' });
        target.focus({ preventScroll: true });
      }
      return;
    }
    const before = anchor.current;
    if (!before || !reading) return;
    if (shown[0]?.id !== before.firstId) {
      element.scrollTop = before.top + element.scrollHeight - before.height;
      anchor.current = null;
      return;
    }
    const added = firstIndex - (total - entries.length);
    if (added > 0) setSnapshot({ ...reading, entries: [...entries.slice(0, added), ...shown] });
  }, [roomId, entries, total, visible, focusEntryId, focusRequestId, focusPending, reading, firstIndex, shown]);

  function trackScroll() {
    const element = scroll.current;
    if (!element || !visible || focusPending || anchor.current) return;
    if (element.scrollHeight - element.scrollTop - element.clientHeight <= 4) {
      if (reading) setSnapshot(null);
    } else if (!reading) setSnapshot({ entries, total, request: focusRequestId });
  }

  async function loadEarlier(load: (before: number) => Promise<boolean>) {
    const element = scroll.current;
    if (!element || anchor.current) return;
    anchor.current = { height: element.scrollHeight, top: element.scrollTop, firstId: shown[0]?.id ?? '' };
    if (!reading) setSnapshot({ entries, total, request: focusRequestId });
    if (!await load(firstIndex)) anchor.current = null;
  }

  function followLatest() {
    anchor.current = null;
    setSnapshot(null);
    if (scroll.current) scroll.current.scrollTop = scroll.current.scrollHeight;
  }
  return { scroll, shown, shownTotal, unread: total - shownTotal, trackScroll, loadEarlier, followLatest };
}
