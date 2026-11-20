import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { AppSnapshot, HistoryPoint, OverlayHitRegion, OverlaySide, OverlayTarget, SessionDetail, Settings } from './types';

const params = new URLSearchParams(window.location.search);
export const demoMode = params.get('demo') === '1';
export const desktopMode = isTauri();
export const initialSnapshot: AppSnapshot = {
  sessions: null, sessionError: null, quota: null, status: 'connecting', error: null, locked: false, overlayVisible: true,
  settings: { distro: '', pollIntervalSecs: 60 }, version: '0.1.8',
};
const now = Math.floor(Date.now() / 1000);
const demoSession = (id: string, title: string, project: string, completionId: string | null = null): SessionDetail =>
  ({ id, title, project, completionId, workingDirectory: null });
const workingDetails = [
  demoSession('demo-1', '核对额度采样记录', 'Codex Weekly Overlay'),
  demoSession('demo-2', '梳理异常处理流程', '应用文档'),
  demoSession('demo-3', '检查回归测试', 'Codex Weekly Overlay'),
  demoSession('demo-4', '整理发布说明', '应用文档'),
];
const unreadCompletedDetails = [
  demoSession('demo-5', '更新面板交互', 'Codex Weekly Overlay', 'turn-5'),
  demoSession('demo-6', '补充使用手册', '应用文档', 'turn-6'),
];
let demoSnapshot: AppSnapshot = {
  ...initialSnapshot, status: 'online',
  sessions: { working: workingDetails.length, unreadCompleted: unreadCompletedDetails.length, observedAt: now, workingDetails, unreadCompletedDetails },
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
export async function setOverlayDetails(expanded: boolean): Promise<OverlaySide> {
  return demoMode || !desktopMode ? 'right' : command('set_overlay_details', { expanded });
}
export async function setOverlayHitRegions(regions: OverlayHitRegion[]): Promise<void> {
  await command('set_overlay_hit_regions', { regions });
}

type OverlayEvents = {
  'overlay-details-side': OverlaySide;
  'overlay-region-hover': OverlayTarget | null;
  'overlay-region-scroll': { deltaY: number };
};
export function subscribeOverlayEvent<K extends keyof OverlayEvents>(name: K, callback: (payload: OverlayEvents[K]) => void) {
  return listen<OverlayEvents[K]>(name, event => callback(event.payload));
}

/** 浏览器与演示模式只做离屏验证，不向桌面托盘写入。 */
export async function setTrayImage(rgba: Uint8ClampedArray): Promise<void> {
  if (demoMode || !desktopMode) return;
  await command('set_tray_image', { rgba: Array.from(rgba) });
}
