import { test } from 'node:test';
import assert from 'node:assert/strict';
import { trayPercent, TRAY_IMAGE_SIZE } from './trayImage.ts';

test('托盘显示整数余量且未知读数保持未知', () => {
  assert.equal(trayPercent(63.6), 64);
  assert.equal(trayPercent(63.4), 63);
  assert.equal(trayPercent(undefined), undefined);
  assert.equal(trayPercent(NaN), undefined);
  assert.equal(trayPercent(Infinity), undefined);
  assert.equal(TRAY_IMAGE_SIZE, 64);
});
test('托盘余量遵循主圆环的百分比边界', () => {
  assert.equal(trayPercent(-1), 0);
  assert.equal(trayPercent(101), 100);
});
