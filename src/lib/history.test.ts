import { test } from 'node:test';
import assert from 'node:assert/strict';
import { historyBridges, historyDomain, splitHistory } from './history.ts';
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

test('截图中的多段平台与单点用间隙线串联，且不改动真实采样', () => {
  const reset = 200000;
  const points = [point(0, 65, 'a'), point(180, 65, 'a'), point(240, 65, 'b'), point(300, 65, 'b'), point(360, 64, 'c'), point(420, 64, 'd'), point(540, 64, 'd'), point(600, 62, 'e'), point(900, 60, 'e')].map(p => ({ ...p, resetsAt: reset }));
  const bridges = historyBridges(splitHistory(points));
  assert.deepEqual(bridges.map(pair => pair.map(p => p.recordedAt)), [[180, 240], [300, 360], [360, 420], [540, 600]]);
  assert.equal(points.length, 9);
  assert.equal(bridges[0][0], points[1]);
});
test('窗口未知、重置变化或剩余额度回升时不跨段连接', () => {
  for (const [firstReset, secondReset, nextValue] of [[null, null, 64], [200000, null, 64], [200000, 300000, 64], [200000, 200000, 66]] as const) {
    assert.deepEqual(historyBridges(splitHistory([{ ...point(0, 65, 'a'), resetsAt: firstReset }, { ...point(60, nextValue, 'b'), resetsAt: secondReset }])), []);
  }
});
test('同段连续采样不重复添加间隙线', () => assert.deepEqual(historyBridges(splitHistory([point(0), point(60)])), []));
