export type ProviderId = 'chatgpt' | 'antigravity' | 'claude';
export type AudioSource = 'microphone' | 'system';
export type HistorySource = AudioSource | 'assistant' | 'user';

export interface ModelOption { id: string; name: string; contextWindow?: number | null }
export interface AudioDevice { id: string; name: string }
export interface ProviderState {
  connected: boolean;
  loading: boolean;
  models: ModelOption[];
  selectedModel: string | null;
  status: string;
  budgetConfirmed: boolean;
  authPending: boolean;
  catalogSource: string | null;
}
export interface Settings {
  micEnabled: boolean;
  systemEnabled: boolean;
  micDeviceId: string | null;
  systemDeviceId: string | null;
  provider: ProviderId;
  chatgptModel?: string | null;
  antigravityModel?: string | null;
  claudeModel?: string | null;
  antigravityPath: string;
  claudePath?: string;
  pythonPath?: string;
  cudaLibPath?: string;
  hideFromCapture: boolean;
  windowOpacity: number;
}
export interface TranscriptEntry {
  id: string;
  source: HistorySource;
  text: string;
  at: string;
}
export interface RoomInfo {
  id: string;
  title: string;
  pinned: boolean;
  createdAt: string;
  updatedAt: string;
  entryCount: number;
  summary: string;
  summaryStale: boolean;
}
export interface RoomSearchHit { room: RoomInfo; snippet: string; entryId: string | null }
export interface RoomTrashInfo { id: string; title: string; deletedAt: string; entryCount: number | null }
export interface RoomRecoveryInfo {
  id: string;
  primaryError: string | null;
  backup: RoomInfo | null;
  backupError: string | null;
  available: boolean;
}
export interface AppState {
  menuRequestId: number;
  hotkeyError: string | null;
  windowAppearanceApplied: boolean;
  platform?: string;
  rooms: RoomInfo[];
  trashRooms: RoomTrashInfo[];
  deletionNotice: string | null;
  activeRoomId: string | null;
  focusEntryId: string | null;
  roomRecoveries: RoomRecoveryInfo[];
  recoveryNotice: string | null;
  roomTitle: string;
  roomSummary: string;
  summaryStale: boolean;
  summaryBusy: boolean;
  historyTotal: number;
  historyLimit: number;
  contextStatus: string;
  listening: boolean;
  audioReady: boolean;
  audioStatus: string;
  busy: boolean;
  providers: Record<ProviderId, ProviderState>;
  settings: Settings;
  devices: {
    microphones: AudioDevice[];
    system: AudioDevice[];
    defaultMicrophoneId: string | null;
    defaultSystemId: string | null;
  };
  levels: { microphone: number | null; system: number | null };
  transcript: TranscriptEntry[];
  answer: {
    text: string;
    model: string | null;
    streaming: boolean;
    firstTokenMs: number | null;
    totalMs: number | null;
  };
  error: string | null;
}

export type RunCommand = (command: string, args?: Record<string, unknown>) => Promise<boolean>;

const emptyProvider = (): ProviderState => ({ connected: false, loading: false, models: [], selectedModel: null, status: 'Не подключено', budgetConfirmed: false, authPending: false, catalogSource: null });
export const emptyState: AppState = {
  menuRequestId: 0, hotkeyError: null,
  windowAppearanceApplied: false,
  rooms: [], trashRooms: [], deletionNotice: null, activeRoomId: null, focusEntryId: null, roomRecoveries: [], recoveryNotice: null,
  roomTitle: '', roomSummary: '', summaryStale: false,
  summaryBusy: false, historyTotal: 0, historyLimit: 200, contextStatus: '',
  listening: false, audioReady: false, audioStatus: 'Готовлю распознавание речи…', busy: false,
  providers: { chatgpt: emptyProvider(), antigravity: emptyProvider(), claude: emptyProvider() },
  settings: { micEnabled: true, systemEnabled: true, micDeviceId: null, systemDeviceId: null, provider: 'chatgpt', antigravityPath: '', hideFromCapture: true, windowOpacity: 100 },
  devices: { microphones: [], system: [], defaultMicrophoneId: null, defaultSystemId: null },
  levels: { microphone: null, system: null },
  transcript: [], answer: { text: '', model: null, streaming: false, firstTokenMs: null, totalMs: null }, error: null,
};
