import { test } from 'node:test';
import assert from 'node:assert/strict';
import { dateTime, quotaPace, resetLabel } from './format.ts';

const now = 1_800_000_000_000;
const resetAfter = (seconds: number) => now / 1000 + seconds;

test('重置倒计时将剩余完整小时拆为天和小时', () => {
  assert.equal(resetLabel(resetAfter(2 * 86400 + 3 * 3600 + 3599), 'countdown', now), '2天3小时后重置');
  assert.equal(resetLabel(resetAfter(86400), 'countdown', now), '1天0小时后重置');
  assert.equal(resetLabel(resetAfter(23 * 3600), 'countdown', now), '0天23小时后重置');
  assert.equal(resetLabel(resetAfter(3599), 'countdown', now), '0天0小时后重置');
});

test('重置倒计时随当前时间减少，到期后等待服务确认', () => {
  const resetsAt = resetAfter(86400 + 3600);
  assert.equal(resetLabel(resetsAt, 'countdown', now + 3600_000), '1天0小时后重置');
  assert.equal(resetLabel(resetsAt, 'countdown', resetsAt * 1000), '等待新的额度窗口');
  assert.equal(resetLabel(resetsAt, 'countdown', resetsAt * 1000 + 1), '等待新的额度窗口');
});

test('缺失重置时间时保留等待提示', () => {
  assert.equal(resetLabel(null, 'countdown', now), '等待服务返回');
  assert.equal(resetLabel(undefined, 'countdown', now), '等待服务返回');
});

test('默认重置标签继续显示主面板的日期和时间', () => {
  assert.equal(resetLabel(resetAfter(86400), 'date', now), dateTime(resetAfter(86400), true));
});

test('用量平稳包含与剩余时间占比相差正负五个百分点的边界', () => {
  const sample = { recordedAt: now / 1000, windowMinutes: 10080, resetsAt: resetAfter(7 * 86400 * .7) };
  for (const [remainingPercent, label] of [
    [64.9, '用量稍快'], [65, '用量平稳'], [70, '用量平稳'], [75, '用量平稳'], [75.1, '用量稍慢'],
  ] as const) {
    assert.equal(quotaPace({ ...sample, remainingPercent }, now)?.label, label);
  }
});

test('已用额度换算产生的浮点误差不影响平稳边界', () => {
  for (const [seconds, nowOffset, usedPercent] of [[121565, 200, 84.9], [91930, 400, 79.8]]) {
    const sample = { recordedAt: now / 1000, windowMinutes: 10080, resetsAt: resetAfter(seconds), remainingPercent: 100 - usedPercent };
    assert.equal(quotaPace(sample, now + nowOffset)?.label, '用量平稳');
  }
});

test('均匀用量指针按已过时间移动，窗口到期后等待有效重置时间', () => {
  const sample = { recordedAt: now / 1000, remainingPercent: 70, windowMinutes: 10080, resetsAt: resetAfter(7 * 86400) };
  assert.equal(quotaPace(sample, now)?.elapsedFraction, 0);
  assert.equal(quotaPace(sample, now + 86400_000)?.elapsedFraction, 1 / 7);
  assert.equal(quotaPace(sample, now + 7 * 86400_000), undefined);
  assert.equal(quotaPace({ ...sample, resetsAt: null }, now), undefined);
  assert.equal(quotaPace(null, now), undefined);
});
