import { useEffect, useRef, useState } from 'react';
import { beginDrag, openDashboard, setTrayImage } from '../lib/api';
import { dateTime, errorMessage, resetLabel } from '../lib/format';
import { useSnapshot } from '../lib/useSnapshot';
import { QuotaRing } from '../components/QuotaRing';
import { SessionBadge } from '../components/SessionBadge';
import { renderTrayImage, trayPercent } from '../lib/trayImage';

export function Overlay() {
  const { snapshot } = useSnapshot();
  const ringHost = useRef<HTMLDivElement>(null);
  const roundedQuota = trayPercent(snapshot.quota?.remainingPercent);
  const stale = snapshot.status !== 'online';
  const [actionError, setActionError] = useState<string | null>(null);
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    // 离线时也按真实时间更新，避免倒计时依赖额度采集成功。
    const timer = window.setInterval(() => setNow(Date.now()), 60_000);
    return () => window.clearInterval(timer);
  }, []);
  useEffect(() => {
    const svg = ringHost.current?.querySelector<SVGSVGElement>('.quota-ring > svg');
    if (!svg) return;
    const controller = new AbortController();
    void renderTrayImage(svg, roundedQuota, stale, controller.signal).then(rgba => {
      if (!controller.signal.aborted) return setTrayImage(rgba);
    }).catch(error => {
      if (!controller.signal.aborted) setActionError(errorMessage(error));
    });
    return () => controller.abort();
  }, [roundedQuota, stale]);
  const status = snapshot.status === 'online' ? `${dateTime(snapshot.quota?.recordedAt)} 已同步` : snapshot.status === 'connecting' ? '正在连接' : snapshot.quota ? '离线 · 上次读数' : '离线 · 尚无读数';
  return <main className="overlay-window" onContextMenu={event => event.preventDefault()}>
    <div ref={ringHost} className={`overlay-widget ${snapshot.locked ? 'is-locked' : ''}`} title={actionError ?? snapshot.error ?? `${snapshot.locked ? '已锁定，请从托盘解锁' : '拖动调整位置，双击打开面板'} · ${status}`} onPointerDown={event => {
      if (snapshot.locked || event.button !== 0 || event.detail > 1) return;
      void beginDrag().catch(error => setActionError(errorMessage(error)));
    }} onDoubleClick={() => { if (!snapshot.locked) void openDashboard().catch(error => setActionError(errorMessage(error))); }}>
      <QuotaRing value={snapshot.quota?.remainingPercent} variant="overlay" stale={snapshot.status !== 'online'} caption={resetLabel(snapshot.quota?.resetsAt, 'countdown', now)} />
      <SessionBadge count={snapshot.sessions?.working} kind="working" />
      <SessionBadge count={snapshot.sessions?.unreadCompleted} kind="completed" />
      <div className={`overlay-status status-${snapshot.status}`}><span className="status-dot" />{status}</div>
    </div>
  </main>;
}
