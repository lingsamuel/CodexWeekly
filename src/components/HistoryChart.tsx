import { useId, useMemo, useState } from 'react';
import type { KeyboardEvent, PointerEvent } from 'react';
import { area, curveMonotoneX, line } from 'd3-shape';
import type { HistoryPoint } from '../lib/types';
import { historyBridges, historyDomain, splitHistory } from '../lib/history';
import { dateTime, percent } from '../lib/format';
import { Icon } from './Icon';

const width = 900;
const height = 168;
const left = 40;
const right = 12;
const top = 12;
const bottom = 30;
export function HistoryChart({ points, hours, loading }: { points: HistoryPoint[]; hours: number; loading: boolean }) {
  const id = useId().replaceAll(':', '');
  const groups = useMemo(() => splitHistory(points), [points]);
  const validPoints = useMemo(() => groups.flat(), [groups]);
  const [hoverIndex, setHoverIndex] = useState<number | null>(null);
  const { start, end } = useMemo(() => historyDomain(validPoints, hours, Date.now() / 1000), [validPoints, hours]);
  const x = (time: number) => left + (time - start) / (end - start) * (width - left - right);
  const y = (value: number) => top + (100 - value) / 100 * (height - top - bottom);
  const curves = useMemo(() => {
    // 复用 D3 单调插值，经过采样值且不在两点之间制造额外的峰谷。
    const stroke = line<HistoryPoint>().x(point => x(point.recordedAt)).y(point => y(point.remainingPercent)).curve(curveMonotoneX);
    const fill = area<HistoryPoint>().x(point => x(point.recordedAt)).y0(y(0)).y1(point => y(point.remainingPercent)).curve(curveMonotoneX);
    return {
      segments: groups.map(group => ({ group, stroke: stroke(group) ?? '', fill: fill(group) ?? '' })),
      bridges: historyBridges(groups).map(pair => ({ key: `${pair[0].recordedAt}-${pair[1].recordedAt}`, stroke: stroke(pair) ?? '' })),
    };
  }, [groups, start, end]);
  const timeLabel = (time: number) => end - start > 86400
    ? new Intl.DateTimeFormat('zh-CN', { month: 'numeric', day: 'numeric' }).format(time * 1000)
    : end - start <= 300
      ? new Intl.DateTimeFormat('zh-CN', { hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false }).format(time * 1000)
      : dateTime(time);
  const selected = hoverIndex === null ? null : validPoints[Math.min(hoverIndex, validPoints.length - 1)];
  function selectNearest(event: PointerEvent<SVGSVGElement>) {
    const bounds = event.currentTarget.getBoundingClientRect();
    const position = (event.clientX - bounds.left) / bounds.width * width;
    let nearest = 0;
    validPoints.forEach((point, index) => { if (Math.abs(x(point.recordedAt) - position) < Math.abs(x(validPoints[nearest].recordedAt) - position)) nearest = index; });
    setHoverIndex(nearest);
  }
  function navigate(event: KeyboardEvent<SVGSVGElement>) {
    if (event.key === 'ArrowRight' || event.key === 'ArrowLeft') {
      event.preventDefault();
      const direction = event.key === 'ArrowRight' ? 1 : -1;
      setHoverIndex(index => Math.max(0, Math.min(validPoints.length - 1, (index ?? (direction > 0 ? -1 : validPoints.length)) + direction)));
    } else if (event.key === 'Home' || event.key === 'End') {
      event.preventDefault();
      setHoverIndex(event.key === 'Home' ? 0 : validPoints.length - 1);
    } else if (event.key === 'Escape') setHoverIndex(null);
  }
  if (validPoints.length === 0) return <div className="chart-empty" role="status"><div className="empty-icon"><Icon name="chart" size={24} /></div><strong>{loading ? '正在读取额度记录' : '从第一次同步开始，记录你的节奏'}</strong><p>{loading ? '稍等片刻，历史记录即将呈现。' : '这个时间范围还没有记录。成功同步后，真实额度变化会显示在这里。'}</p></div>;
  return <div className="chart-wrap">
    <svg className="history-chart" viewBox={`0 0 ${width} ${height}`} role="img" tabIndex={0} aria-label={`最近 ${hours} 小时的额度曲线，共 ${validPoints.length} 个读数。淡绿虚线代表采样间隙。使用左右方向键逐点查看。`} aria-describedby={`${id}-detail`} onPointerMove={selectNearest} onPointerLeave={() => setHoverIndex(null)} onKeyDown={navigate} onBlur={() => setHoverIndex(null)}>
      <defs><linearGradient id={`${id}-area`} x1="0" y1="0" x2="0" y2="1"><stop stopColor="#80cfad" stopOpacity=".23" /><stop offset="1" stopColor="#a8ddbb" stopOpacity=".015" /></linearGradient></defs>
      {[100, 75, 50, 25, 0].map(value => <g key={value}><line x1={left} y1={y(value)} x2={width - right} y2={y(value)} stroke="#e8eee6" strokeDasharray="3 5" /><text className="chart-axis" x={left - 12} y={y(value) + 4} textAnchor="end">{value}</text></g>)}
      {curves.bridges.map(bridge => <path key={bridge.key} className="history-gap" d={bridge.stroke} fill="none" stroke="#80bca0" strokeWidth="2" strokeDasharray="4 4" strokeLinecap="round" opacity=".7" />)}
      {curves.segments.map(({ group, stroke, fill }, index) => {
        const first = group[0], last = group[group.length - 1];
        return <g key={`${first.segment}-${index}`}>
          {group.length > 1 && <path className="history-area" d={fill} fill={`url(#${id}-area)`} />}
          {group.length > 1 && <path className="history-line" d={stroke} fill="none" stroke="#56ad8e" strokeWidth="2.4" strokeLinejoin="round" strokeLinecap="round" />}
          <circle cx={x(last.recordedAt)} cy={y(last.remainingPercent)} r="3.8" fill="#58b18f" stroke="white" strokeWidth="2" />
        </g>;
      })}
      {[0, 1, 2, 3, 4].map(index => {
        const time = start + (end - start) * index / 4;
        return <text key={index} className="chart-axis" x={x(time)} y={height - 6} textAnchor={index === 0 ? 'start' : index === 4 ? 'end' : 'middle'}>{timeLabel(time)}</text>;
      })}
      {selected && <g><line x1={x(selected.recordedAt)} x2={x(selected.recordedAt)} y1={top} y2={y(0)} stroke="#8cb6a1" strokeDasharray="3 4" /><circle cx={x(selected.recordedAt)} cy={y(selected.remainingPercent)} r="5" fill="#459b7a" stroke="white" strokeWidth="2" /></g>}
    </svg>
    {validPoints.length === 1 && <p className="chart-hint" role="status">已有 1 次采样，下一次成功同步后会绘制连续曲线。</p>}
    {validPoints.length > 1 && curves.bridges.length === 0 && groups.every(group => group.length === 1) && <p className="chart-hint" role="status">当前记录来自不同的采集时段；连续采样后会显示分段曲线。</p>}
    <div id={`${id}-detail`} role="status" aria-live="polite" className={`chart-tooltip ${selected ? 'is-visible' : ''}`} style={{ left: selected ? `${Math.max(17, Math.min(83, x(selected.recordedAt) / width * 100))}%` : '50%' }}>
      {selected && <><span>{dateTime(selected.recordedAt, true)}</span><strong>剩余 {percent(selected.remainingPercent)}%</strong></>}
    </div>
  </div>;
}
