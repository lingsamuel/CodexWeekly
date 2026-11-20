import type { ComponentPropsWithRef } from 'react';
import type { SessionKind } from '../lib/types';

type SessionBadgeProps = Pick<ComponentPropsWithRef<'span'>, 'ref' | 'onPointerEnter' | 'onPointerLeave' | 'onPointerDown'> & {
  count: number | null | undefined;
  kind: SessionKind;
};

export function SessionBadge({ count, kind, ...interaction }: SessionBadgeProps) {
  if (count == null || !Number.isFinite(count) || count <= 0) return null;
  const label = `${count} 个${kind === 'working' ? '正在工作中的会话' : '本次启动后完成且回执未读的会话'}`;
  return <span {...interaction} className={`session-badge session-badge--${kind}`} title={label} role="img" aria-label={label}
    onDoubleClick={event => event.stopPropagation()}>{count > 99 ? '99+' : Math.floor(count)}</span>;
}
