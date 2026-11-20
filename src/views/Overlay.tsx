import { useEffect, useEffectEvent, useLayoutEffect, useRef, useState } from 'react';
import type { PointerEvent } from 'react';
import { beginDrag, demoMode, desktopMode, openDashboard, setOverlayHitRegions, setTrayImage, subscribeOverlayEvent } from '../lib/api';
import { dateTime, errorMessage, resetLabel } from '../lib/format';
import { useOverlayDetailsWindow } from '../lib/useOverlayDetailsWindow';
import { useSessionDetails } from '../lib/useSessionDetails';
import type { SessionGroup } from '../lib/useSessionDetails';
import { useSnapshot } from '../lib/useSnapshot';
import { QuotaRing } from '../components/QuotaRing';
import { SessionBadge } from '../components/SessionBadge';
import { SessionDetails } from '../components/SessionDetails';
import { renderTrayImage, trayPercent } from '../lib/trayImage';
import type { OverlayTarget } from '../lib/types';

const nativeInput = desktopMode && !demoMode;

export function Overlay() {
  const { snapshot } = useSnapshot();
  const ringHost = useRef<HTMLDivElement>(null);
  const workingBadge = useRef<HTMLSpanElement>(null);
  const completedBadge = useRef<HTMLSpanElement>(null);
  const detailsRegion = useRef<HTMLElement>(null);
  const detailsScroll = useRef<HTMLDivElement>(null);
  const lastHitRegions = useRef('');
  const roundedQuota = trayPercent(snapshot.quota?.remainingPercent);
  const stale = snapshot.status !== 'online';
  const [actionError, setActionError] = useState<string | null>(null);
  const showError = (error: unknown) => setActionError(errorMessage(error));
  const { groups, hover } = useSessionDetails(snapshot.sessions, snapshot.overlayVisible, snapshot.settings.distro);
  const wantsDetails = groups.length > 0;
  const { expanded, side, setSide } = useOverlayDetailsWindow(wantsDetails, snapshot.overlayVisible, showError);
  const panelVisible = expanded && wantsDetails;
  const [lastGroups, setLastGroups] = useState<SessionGroup[]>([]);
  const workingCount = snapshot.sessions?.working;
  const completedCount = snapshot.sessions?.unreadCompleted;
  const [now, setNow] = useState(Date.now);
  const nativeHover = useEffectEvent((target: OverlayTarget | null) => {
    // 原生区域可能先于锁定状态广播更新，直接采用命中结果。
    hover(target, side === 'left');
  });
  const nativeScroll = useEffectEvent(({ deltaY }: { deltaY: number }) => {
    if (panelVisible && detailsScroll.current) detailsScroll.current.scrollTop += deltaY;
  });
  useEffect(() => {
    if (groups.length > 0) setLastGroups(groups);
  }, [groups]);
  useEffect(() => {
    if (!nativeInput) return;
    let active = true;
    const releases: (() => void)[] = [];
    const subscriptions = [
      subscribeOverlayEvent('overlay-details-side', side => { if (active) setSide(side); }),
      subscribeOverlayEvent('overlay-region-hover', target => { if (active) nativeHover(target); }),
      subscribeOverlayEvent('overlay-region-scroll', payload => { if (active) nativeScroll(payload); }),
    ];
    for (const subscription of subscriptions) {
      void subscription.then(release => {
        if (active) releases.push(release);
        else release();
      }).catch(error => { if (active) setActionError(errorMessage(error)); });
    }
    return () => { active = false; releases.forEach(release => release()); };
  }, [setSide]);
  useLayoutEffect(() => {
    if (!nativeInput) return;
    const targets: [OverlayTarget, HTMLElement | null][] = snapshot.overlayVisible ? [
      ['working', workingBadge.current],
      ['completed', completedBadge.current],
      ['details', panelVisible ? detailsRegion.current : null],
    ] : [];
    const publish = () => {
      const regions = targets.flatMap(([target, element]) => {
        if (!element) return [];
        const { x, y, width, height } = element.getBoundingClientRect();
        return [{ target, x, y, width, height }];
      });
      const serialized = JSON.stringify(regions);
      if (serialized === lastHitRegions.current) return;
      lastHitRegions.current = serialized;
      void setOverlayHitRegions(regions).catch(error => {
        if (lastHitRegions.current === serialized) lastHitRegions.current = '';
        setActionError(errorMessage(error));
      });
    };
    publish();
    const observer = new ResizeObserver(publish);
    for (const [, element] of targets) if (element) observer.observe(element);
    window.addEventListener('resize', publish);
    return () => { observer.disconnect(); window.removeEventListener('resize', publish); };
  }, [snapshot.overlayVisible, side, panelVisible, workingCount, completedCount, groups]);
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
  function startDrag(event: PointerEvent) {
    if (snapshot.locked || event.button !== 0 || event.detail > 1) return;
    void beginDrag().catch(showError);
  }
  function startBadgeDrag(event: PointerEvent) {
    event.stopPropagation();
    startDrag(event);
  }
  function hoverFromDom(target: OverlayTarget | null) {
    if (!nativeInput || !snapshot.locked) hover(target, side === 'left');
  }
  return <main className={`overlay-window overlay-window--${side}`} hidden={!snapshot.overlayVisible} onContextMenu={event => event.preventDefault()}>
    <div ref={ringHost} className={`overlay-widget ${snapshot.locked ? 'is-locked' : ''}`}
      title={actionError ?? snapshot.error ?? `${snapshot.locked ? '已锁定，可从会话气泡拖动' : '拖动调整位置，双击打开面板'} · ${status}`}
      onPointerDown={startDrag} onDoubleClick={() => { if (!snapshot.locked) void openDashboard().catch(showError); }}>
      <QuotaRing value={snapshot.quota?.remainingPercent} variant="overlay" stale={snapshot.status !== 'online'} caption={resetLabel(snapshot.quota?.resetsAt, 'countdown', now)} />
      <SessionBadge ref={workingBadge} count={workingCount} kind="working" onPointerDown={startBadgeDrag}
        onPointerEnter={() => hoverFromDom('working')} onPointerLeave={() => hoverFromDom(null)} />
      <SessionBadge ref={completedBadge} count={completedCount} kind="completed" onPointerDown={startBadgeDrag}
        onPointerEnter={() => hoverFromDom('completed')} onPointerLeave={() => hoverFromDom(null)} />
      <div className={`overlay-status status-${snapshot.status}`}><span className="status-dot" />{status}</div>
    </div>
    {expanded && <SessionDetails groups={wantsDetails ? groups : lastGroups} visible={panelVisible} regionRef={detailsRegion} scrollRef={detailsScroll}
      onPointerEnter={() => hoverFromDom('details')} onPointerLeave={() => hoverFromDom(null)} />}
  </main>;
}
