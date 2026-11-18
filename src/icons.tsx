import type { CSSProperties } from 'react';

export type IconName = 'mic' | 'speaker' | 'play' | 'stop' | 'spark' | 'settings' | 'refresh' | 'close' | 'copy' | 'check' | 'arrow' | 'shield' | 'trash' | 'chevron' | 'search' | 'plus' | 'pin' | 'edit' | 'folder' | 'file' | 'wave' | 'sidebar';
const paths: Record<IconName, string> = {
  sidebar: 'M3 5h18v14H3zM9 5v14',
  wave: 'M3 10v4m4-8v12m5-16v20m5-16v12m4-8v4',
  mic: 'M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3ZM6 10v2a6 6 0 0 0 12 0v-2M12 18v3M8 21h8',
  speaker: 'M11 5 6 9H3v6h3l5 4V5ZM15 8a6 6 0 0 1 0 8M18 5a10 10 0 0 1 0 14',
  play: 'm9 5 10 7-10 7V5Z',
  stop: 'M6 6h12v12H6z',
  spark: 'm12 3 2.4 6.6L21 12l-6.6 2.4L12 21l-2.4-6.6L3 12l6.6-2.4L12 3Z',
  settings: 'M4 7h16M4 17h16M8 4v6M16 14v6',
  refresh: 'M20 7v5h-5M4 17v-5h5M6.1 7a7 7 0 0 1 11.5-1L20 9M4 15l2.4 3A7 7 0 0 0 18 17',
  close: 'm6 6 12 12M18 6 6 18',
  copy: 'M8 8h12v13H8zM16 8V3H3v13h5',
  check: 'm5 12 4 4L19 6',
  arrow: 'M5 12h14m-6-6 6 6-6 6',
  shield: 'm12 3 8 3v6c0 5-8 9-8 9s-8-4-8-9V6l8-3Zm-4 9 3 3 5-6',
  trash: 'M3 6h18M9 6V3h6v3M6 6l1 15h10l1-15M10 10v7M14 10v7',
  chevron: 'm6 9 6 6 6-6',
  search: 'M20 20l-5-5M10.5 4a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13Z',
  plus: 'M12 5v14M5 12h14',
  pin: 'm8 3 8 0-1 6 4 4H5l4-4-1-6ZM12 13v8',
  edit: 'm15 4 5 5M4 20l5-1L20 8a2 2 0 0 0-4-4L5 15l-1 5Z',
  folder: 'M3 7V5a2 2 0 0 1 2-2h5l2 3h7a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Z',
  file: 'M14 2H5a1 1 0 0 0-1 1v18a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1V8l-6-6ZM14 2v6h6M8 13h8M8 17h6',
};

export function Icon({ name, size = 20, style }: { name: IconName; size?: number; style?: CSSProperties }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.65" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" style={style}><path d={paths[name]} /></svg>;
}
