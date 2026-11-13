import { test } from 'node:test';
import assert from 'node:assert/strict';
import { dateTime, resetLabel } from './format.ts';

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
