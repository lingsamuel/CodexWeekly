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
export function resetLabel(seconds: number | null | undefined): string {
  if (seconds == null) return '等待服务返回';
  if (seconds * 1000 <= Date.now()) return '等待新的额度窗口';
  return dateTime(seconds, true);
}
export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : typeof error === 'string' ? error : '连接遇到问题，请稍后重试。';
}
