export interface Settings {
  distro: string;
  pollIntervalSecs: number;
}
export interface QuotaSample {
  recordedAt: number;
  remainingPercent: number;
  resetsAt: number | null;
  windowMinutes: number;
}
export interface HistoryPoint extends QuotaSample {
  segment: string;
}
export interface AppSnapshot {
  quota: QuotaSample | null;
  status: 'connecting' | 'online' | 'offline';
  error: string | null;
  locked: boolean;
  overlayVisible: boolean;
  settings: Settings;
  version: string;
}
