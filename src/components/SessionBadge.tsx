export function SessionBadge({ count, kind }: { count: number | null | undefined; kind: 'working' | 'completed' }) {
  if (count == null || !Number.isFinite(count) || count <= 0) return null;
  const label = `${count} 个${kind === 'working' ? '正在工作中的会话' : '已完成但尚未阅读的会话'}`;
  return <span className={`session-badge session-badge--${kind}`} title={label} role="img" aria-label={label}>{count > 99 ? '99+' : Math.floor(count)}</span>;
}
