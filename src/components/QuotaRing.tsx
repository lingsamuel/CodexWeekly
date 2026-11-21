import { useId } from 'react';
import { percent } from '../lib/format';

const dayAngles = Array.from({ length: 7 }, (_, day) => day * 360 / 7);

export function QuotaRing({ value, variant = 'dashboard', stale = false, syncFailed = false, caption = '可用额度', elapsedFraction }: {
  value?: number;
  variant?: 'overlay' | 'dashboard';
  stale?: boolean;
  syncFailed?: boolean;
  caption?: string;
  elapsedFraction?: number;
}) {
  const id = useId().replaceAll(':', '');
  const safeValue = value !== undefined && Number.isFinite(value) ? Math.max(0, Math.min(100, value)) : undefined;
  const circumference = 2 * Math.PI * 68;
  const progressLength = circumference * (safeValue ?? 0) / 100;
  const syncLabel = variant === 'overlay' ? '' : stale ? '，上次有效读数' : '，已同步';
  return <div className={`quota-ring quota-ring--${variant} ${stale ? 'is-stale' : ''} ${syncFailed ? 'is-sync-failed' : ''}`} role="img" aria-label={safeValue === undefined ? 'Weekly 额度尚未同步' : `Weekly 剩余 ${percent(safeValue)}%${syncLabel}`}>
    <svg viewBox="0 0 168 168" aria-hidden="true">
      <defs>
        <linearGradient id={`${id}-ring`} className="ring-value-gradient" x1="0" y1="1" x2="1" y2="0"><stop stopColor="#52b59e" /><stop offset=".55" stopColor="#80d6b3" /><stop offset="1" stopColor="#b8e7ad" /></linearGradient>
        <radialGradient id={`${id}-fill`}><stop stopColor="#ffffff" /><stop offset="1" stopColor="#f0f8ed" /></radialGradient>
      </defs>
      <circle cx="84" cy="84" r="77" fill={`url(#${id}-fill)`} />
      <circle className="ring-track" cx="84" cy="84" r="68" />
      {/* 固定余量弧的末端在 12 点，使已用部分从右上方顺时针扩展。 */}
      {safeValue !== undefined && <circle className="ring-value" cx="84" cy="84" r="68" stroke={`url(#${id}-ring)`} strokeDasharray={`${progressLength} ${circumference}`} strokeDashoffset={progressLength - circumference} transform="rotate(-90 84 84)" />}
      {variant === 'overlay' && <g className="ring-window-scale">
        {dayAngles.map(angle => <line key={angle} className="ring-day-tick" x1="84" x2="84" y1="21.5" y2="24" transform={`rotate(${angle} 84 84)`} />)}
        {/* 指针与余量弧共用起点和方向，直接对齐已用额度与已过时间。 */}
        {elapsedFraction !== undefined && <path className="ring-pace-pointer" d="M84 16 L88 24 L84 29 L80 24 Z" transform={`rotate(${elapsedFraction * 360} 84 84)`} />}
      </g>}
    </svg>
    <div className="ring-copy"><span className="ring-eyebrow">WEEKLY</span><div className="ring-number">{percent(safeValue)}{safeValue !== undefined && <span>%</span>}</div><span className="ring-caption">{caption}</span></div>
  </div>;
}
