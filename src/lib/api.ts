import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { AppSnapshot, HistoryPoint, Settings } from './types';

const params = new URLSearchParams(window.location.search);
export const demoMode = params.get('demo') === '1';
export const desktopMode = isTauri();
export const initialSnapshot: AppSnapshot = {
  quota: null, status: 'connecting', error: null, locked: false, overlayVisible: true,
  settings: { distro: '', pollIntervalSecs: 60 }, version: '0.1.0',
};
const now = Math.floor(Date.now() / 1000);
let demoSnapshot: AppSnapshot = {
  ...initialSnapshot, status: 'online',
  quota: { recordedAt: now, remainingPercent: 73, resetsAt: now + 60 * 60 * 24 * 3, windowMinutes: 10080 },
};
const demoListeners = new Set<(snapshot: AppSnapshot) => void>();
function publishDemo(): AppSnapshot {
  demoListeners.forEach(listener => listener(demoSnapshot));
  return demoSnapshot;
}

async function command<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  if (!desktopMode) throw new Error('请在桌面应用中运行。浏览器无法连接 WSL 与本地额度记录。');
  return invoke<T>(name, args);
}
export async function getSnapshot(): Promise<AppSnapshot> {
  return demoMode ? demoSnapshot : command('get_snapshot');
}
export async function subscribeState(callback: (snapshot: AppSnapshot) => void): Promise<() => void> {
  if (demoMode) {
    demoListeners.add(callback);
    return () => { demoListeners.delete(callback); };
  }
  if (!desktopMode) return () => {};
  return listen<AppSnapshot>('state-changed', event => callback(event.payload));
}
export async function getHistory(hours: number): Promise<HistoryPoint[]> {
  if (!demoMode) return command('get_history', { hours });
  // 演示数据仅在明确 ?demo=1 时生成，生产模式始终读取 SQLite。
  const count = 96;
  return Array.from({ length: count }, (_, index) => {
    const progress = index / (count - 1);
    return {
      recordedAt: now - hours * 3600 * (1 - progress),
      remainingPercent: Math.round((94 - 21 * progress + Math.sin(progress * 15) * (1 - progress) * 1.3) * 10) / 10,
      resetsAt: demoSnapshot.quota!.resetsAt, windowMinutes: 10080, segment: 'demo-window',
    };
  });
}
export async function updateSettings(settings: Settings): Promise<AppSnapshot> {
  if (!demoMode) return command('update_settings', { settings });
  demoSnapshot = { ...demoSnapshot, settings };
  return publishDemo();
}
export async function refresh(): Promise<void> {
  if (!demoMode) return command('refresh');
  demoSnapshot = { ...demoSnapshot, quota: { ...demoSnapshot.quota!, recordedAt: Math.floor(Date.now() / 1000) } };
  publishDemo();
}
export async function setLocked(locked: boolean): Promise<AppSnapshot> {
  if (!demoMode) return command('set_locked', { locked });
  demoSnapshot = { ...demoSnapshot, locked };
  return publishDemo();
}
export async function setOverlayVisible(visible: boolean): Promise<AppSnapshot> {
  if (!demoMode) return command('set_overlay_visible', { visible });
  demoSnapshot = { ...demoSnapshot, overlayVisible: visible };
  return publishDemo();
}
export async function listDistros(): Promise<string[]> {
  return demoMode ? ['Ubuntu', 'Debian'] : command('list_distros');
}
export async function beginDrag(): Promise<void> {
  if (!demoMode) return command('begin_drag');
}
export async function openDashboard(): Promise<void> {
  if (!demoMode) return command('open_dashboard');
}
