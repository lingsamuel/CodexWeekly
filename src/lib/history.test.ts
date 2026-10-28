import { test } from 'node:test';
import assert from 'node:assert/strict';
import { splitHistory } from './history.ts';
import type { HistoryPoint } from './types.ts';
const point = (recordedAt: number, remainingPercent = 73, segment = 'a'): HistoryPoint => ({ recordedAt, remainingPercent, segment, resetsAt: null, windowMinutes: 10080 });

test('空历史不生成假点', () => assert.deepEqual(splitHistory([]), []));
test('时间乱序时依时间排列，且不修改原数组', () => {
  const points = [point(120), point(60)];
  assert.deepEqual(splitHistory(points).map(group => group.map(p => p.recordedAt)), [[60, 120]]);
  assert.equal(points[0].recordedAt, 120);
});
test('重置分段之间不连线', () => assert.equal(splitHistory([point(0, 20), point(60, 100, 'b')]).length, 2));
test('长范围降采样不会被误判为断线', () => assert.equal(splitHistory([point(0), point(60), point(600)]).length, 1));
test('非法时间与额度不进入图表', () => {
  const groups = splitHistory([point(NaN), point(0, Infinity), point(1, -1), point(2, 101), point(3, 0)]);
  assert.deepEqual(groups.map(group => group.map(p => p.remainingPercent)), [[0]]);
});
