import type { HistoryPoint } from './types.ts';

/** 选择器限定查询范围；记录不足时展开真实采样跨度，避免短线被末端圆点遮住。 */
export function historyDomain(points: HistoryPoint[], hours: number, now: number): { start: number; end: number } {
  if (points.length === 0) return { start: now - hours * 3600, end: now };
  const first = points.reduce((time, point) => Math.min(time, point.recordedAt), Infinity);
  const end = points.reduce((time, point) => Math.max(time, point.recordedAt), now);
  return { start: Math.min(end - 120, Math.max(end - hours * 3600, first)), end };
}

/** 后端在原始采样层识别重置与断线；前端保留分段，避免误判降采样间距。 */
export function splitHistory(points: HistoryPoint[]): HistoryPoint[][] {
  const sorted = points.filter(point =>
    Number.isFinite(point.recordedAt) && Number.isFinite(point.remainingPercent)
    && point.remainingPercent >= 0 && point.remainingPercent <= 100,
  ).sort((a, b) => a.recordedAt - b.recordedAt);
  const groups: HistoryPoint[][] = [];
  for (const point of sorted) {
    const previous = groups.at(-1)?.at(-1);
    if (!previous || previous.segment !== point.segment) {
      groups.push([point]);
    } else {
      groups.at(-1)!.push(point);
    }
  }
  return groups;
}

/** 只提示同一已知额度窗口内的采样间隙；保留端点，不插入虚构读数。 */
export function historyBridges(groups: HistoryPoint[][]): [HistoryPoint, HistoryPoint][] {
  const bridges: [HistoryPoint, HistoryPoint][] = [];
  for (let index = 1; index < groups.length; index++) {
    const previous = groups[index - 1].at(-1);
    const next = groups[index][0];
    if (previous && next && previous.resetsAt !== null && Number.isFinite(previous.resetsAt)
      && previous.resetsAt === next.resetsAt && next.remainingPercent <= previous.remainingPercent
      && next.recordedAt > previous.recordedAt) bridges.push([previous, next]);
  }
  return bridges;
}
