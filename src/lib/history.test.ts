import { test } from 'node:test';
import assert from 'node:assert/strict';
import { historyDomain, orderedHistory } from './history.ts';
import type { HistoryPoint } from './types.ts';
const point = (recordedAt: number, remainingPercent = 73, segment = 'a'): HistoryPoint => ({ recordedAt, remainingPercent, segment, resetsAt: null, windowMinutes: 10080 });

test('空历史不生成假点', () => assert.deepEqual(orderedHistory([]), []));
test('时间乱序时依时间排列，且不修改原数组', () => {
  const points = [point(120), point(60)];
  assert.deepEqual(orderedHistory(points).map(p => p.recordedAt), [60, 120]);
  assert.equal(points[0].recordedAt, 120);
});
test('采集分段和额度变化不拆开展示数据', () => assert.deepEqual(orderedHistory([point(0, 20), point(60, 100, 'b')]).map(p => p.recordedAt), [0, 60]));
test('采样间隔不丢弃中间点', () => assert.equal(orderedHistory([point(0), point(60), point(600)]).length, 3));
test('非法时间与额度不进入图表', () => {
  const points = orderedHistory([point(NaN), point(0, Infinity), point(1, -1), point(2, 101), point(3, 0)]);
  assert.deepEqual(points.map(p => p.remainingPercent), [0]);
});

test('少量历史按真实记录范围展开，不压缩到整天横轴的末端', () => {
  assert.deepEqual(historyDomain([point(99400), point(99940)], 24, 100000), { start: 99400, end: 100000 });
});
test('单点保留最少两分钟横轴跨度', () => {
  assert.deepEqual(historyDomain([point(99990)], 24, 100000), { start: 99880, end: 100000 });
});
test('空数据仍使用所选时间范围', () => {
  assert.deepEqual(historyDomain([], 24, 100000), { start: 13600, end: 100000 });
});
test('实际横轴不超出所选历史范围', () => {
  assert.deepEqual(historyDomain([point(1), point(99940)], 24, 100000), { start: 13600, end: 100000 });
});

test('截图中的多段平台与单点按时间进入同一条曲线', () => {
  const points = [point(0, 65, 'a'), point(180, 65, 'a'), point(240, 65, 'b'), point(300, 65, 'b'), point(360, 64, 'c'), point(420, 64, 'd'), point(540, 64, 'd'), point(600, 62, 'e'), point(900, 60, 'e')];
  const ordered = orderedHistory(points);
  assert.deepEqual(ordered.map(p => p.recordedAt), [0, 180, 240, 300, 360, 420, 540, 600, 900]);
  assert.equal(ordered.length, points.length);
  assert.equal(ordered[2], points[2]);
});
