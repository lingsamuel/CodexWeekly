import type { QuotaSample } from './types';

export function percent(value: number | undefined): string {
  return value === undefined || !Number.isFinite(value) ? '—' : new Intl.NumberFormat('en-US', { maximumFractionDigits: 1 }).format(value);
}
export function dateTime(seconds: number | null | undefined, includeDate = false): string {
  if (seconds == null || !Number.isFinite(seconds)) return '等待首次同步';
  return new Intl.DateTimeFormat('zh-CN', {
    ...(includeDate ? { month: '2-digit', day: '2-digit' } as const : {}),
    hour: '2-digit', minute: '2-digit', hour12: false,
  }).format(new Date(seconds * 1000));
}

function resetRemainingMs(seconds: number | null | undefined, now: number) {
  return seconds == null || !Number.isFinite(seconds) ? undefined : seconds * 1000 - now;
}

// 两种显示共用服务时间和到期规则；倒计时只展示完整小时，不推断额度已重置。
export function resetLabel(seconds: number | null | undefined, format: 'date' | 'countdown' = 'date', now = Date.now()): string {
  const remainingMs = resetRemainingMs(seconds, now);
  if (remainingMs === undefined) return '等待服务返回';
  if (remainingMs <= 0) return '等待新的额度窗口';
  if (format === 'countdown') {
    const hours = Math.floor(remainingMs / 3600_000);
    return `${Math.floor(hours / 24)}天${hours % 24}小时后重置`;
  }
  return dateTime(seconds, true);
}

/** 根据有效窗口定位悬浮窗指针，并用未取整的余量与时间占比判断节奏。 */
export function quotaPace(quota: QuotaSample | null, now: number) {
  if (!quota) return;
  const remainingMs = resetRemainingMs(quota.resetsAt, now);
  const windowMs = quota.windowMinutes * 60_000;
  if (remainingMs === undefined || remainingMs <= 0 || windowMs <= 0) return;

  const timeLeft = Math.min(remainingMs, windowMs);
  const difference = quota.remainingPercent - timeLeft * 100 / windowMs;
  // 额度和时间占比各自换算时可能产生浮点误差，保留平稳区间的端点。
  const tolerance = 5 + 100 * Number.EPSILON;
  const label = difference < -tolerance ? '用量稍快' : difference > tolerance ? '用量稍慢' : '用量平稳';
  return { elapsedFraction: (windowMs - timeLeft) / windowMs, label };
}

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : typeof error === 'string' ? error : '连接遇到问题，请稍后重试。';
}
