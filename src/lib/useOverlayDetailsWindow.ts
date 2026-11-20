import { useEffect, useEffectEvent, useRef, useState } from 'react';
import { setOverlayDetails } from './api';
import type { OverlaySide } from './types';

export function useOverlayDetailsWindow(wantsDetails: boolean, visible: boolean, onError: (error: unknown) => void) {
  const [expanded, setExpanded] = useState(false);
  const [side, setSide] = useState<OverlaySide>('right');
  const requested = useRef(false);
  const pending = useRef(Promise.resolve());

  const resize = useEffectEvent((target: boolean) => {
    if (requested.current === target) return;
    requested.current = target;
    // 快速移出后又移入时，按顺序应用原生窗口尺寸。
    pending.current = pending.current.then(async () => {
      if (requested.current !== target) return;
      try {
        const placement = await setOverlayDetails(target);
        setSide(placement);
        setExpanded(target);
      } catch (error) {
        if (requested.current === target) requested.current = !target;
        onError(error);
      }
    });
  });

  useEffect(() => {
    if (wantsDetails || !visible || window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      resize(wantsDetails);
      return;
    }
    // 保留扩展区域直至 150 ms 的退场动画绘制完毕。
    const timer = window.setTimeout(() => resize(false), 180);
    return () => window.clearTimeout(timer);
  }, [wantsDetails, visible]);

  return { expanded, side: expanded ? side : 'right', setSide };
}
