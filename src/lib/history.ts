import type { HistoryPoint } from './types.ts';

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
