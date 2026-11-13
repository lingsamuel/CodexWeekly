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
// 两种显示共用服务时间和到期规则；倒计时只展示完整小时，不推断额度已重置。
export function resetLabel(seconds: number | null | undefined, format: 'date' | 'countdown' = 'date', now = Date.now()): string {
  if (seconds == null) return '等待服务返回';
  if (seconds * 1000 <= now) return '等待新的额度窗口';
  if (format === 'countdown') {
    const hours = Math.floor((seconds * 1000 - now) / 3600_000);
    return `${Math.floor(hours / 24)}天${hours % 24}小时后重置`;
  }
  return dateTime(seconds, true);
}
export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : typeof error === 'string' ? error : '连接遇到问题，请稍后重试。';
}
