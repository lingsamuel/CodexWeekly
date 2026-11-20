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
export type SessionKind = 'working' | 'completed';
export interface SessionDetail {
  id: string;
  completionId: string | null;
  title: string | null;
  project: string | null;
  workingDirectory: string | null;
}
export interface SessionCounts {
  working: number;
  unreadCompleted: number | null;
  observedAt: number;
  workingDetails: SessionDetail[];
  unreadCompletedDetails: SessionDetail[] | null;
}
export type OverlaySide = 'left' | 'right';
export type OverlayTarget = SessionKind | 'details';
export interface OverlayHitRegion {
  target: OverlayTarget;
  x: number;
  y: number;
  width: number;
  height: number;
}
export interface AppSnapshot {
  sessions: SessionCounts | null;
  sessionError: string | null;
  quota: QuotaSample | null;
  status: 'connecting' | 'online' | 'offline';
  error: string | null;
  locked: boolean;
  overlayVisible: boolean;
  settings: Settings;
  version: string;
}
