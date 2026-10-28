import type { HistoryPoint } from './types.ts';

/** 选择器限定查询范围；记录不足时展开真实采样跨度，避免短线被末端圆点遮住。 */
export function historyDomain(points: HistoryPoint[], hours: number, now: number): { start: number; end: number } {
  if (points.length === 0) return { start: now - hours * 3600, end: now };
  const first = points.reduce((time, point) => Math.min(time, point.recordedAt), Infinity);
  const end = points.reduce((time, point) => Math.max(time, point.recordedAt), now);
  return { start: Math.min(end - 120, Math.max(end - hours * 3600, first)), end };
}

/** 展示按时间连续拟合，采集分段不再拆开曲线和面积。 */
export function orderedHistory(points: HistoryPoint[]): HistoryPoint[] {
  return points.filter(point =>
    Number.isFinite(point.recordedAt) && Number.isFinite(point.remainingPercent)
    && point.remainingPercent >= 0 && point.remainingPercent <= 100,
  ).sort((a, b) => a.recordedAt - b.recordedAt);
}
