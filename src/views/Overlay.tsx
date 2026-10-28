import { useState } from 'react';
import { beginDrag, openDashboard } from '../lib/api';
import { dateTime, errorMessage } from '../lib/format';
import { useSnapshot } from '../lib/useSnapshot';
import { QuotaRing } from '../components/QuotaRing';

export function Overlay() {
  const { snapshot } = useSnapshot();
  const [actionError, setActionError] = useState<string | null>(null);
  const status = snapshot.status === 'online' ? `${dateTime(snapshot.quota?.recordedAt)} 已同步` : snapshot.status === 'connecting' ? '正在连接' : snapshot.quota ? '离线 · 上次读数' : '离线 · 尚无读数';
  return <main className="overlay-window">
    <div className={`overlay-widget ${snapshot.locked ? 'is-locked' : ''}`} title={actionError ?? snapshot.error ?? `${snapshot.locked ? '已锁定，请从托盘解锁' : '拖动调整位置，双击打开面板'} · ${status}`} onPointerDown={event => {
      if (snapshot.locked || event.button !== 0 || event.detail > 1) return;
      void beginDrag().catch(error => setActionError(errorMessage(error)));
    }} onDoubleClick={() => { if (!snapshot.locked) void openDashboard().catch(error => setActionError(errorMessage(error))); }}>
      <QuotaRing value={snapshot.quota?.remainingPercent} variant="overlay" stale={snapshot.status !== 'online'} />
      <div className={`overlay-status status-${snapshot.status}`}><span className="status-dot" />{status}</div>
    </div>
  </main>;
}
