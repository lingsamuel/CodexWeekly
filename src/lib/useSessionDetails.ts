import { useEffect, useMemo, useRef, useState } from 'react';
import type { OverlayTarget, SessionCounts, SessionDetail, SessionKind } from './types';

const kinds = ['working', 'completed'] as const;
const AUTO_OPEN_MS = 8_000;
const NO_KINDS: SessionKind[] = [];

export interface SessionGroup {
  kind: SessionKind;
  sessions: SessionDetail[];
}

export function useSessionDetails(sessions: SessionCounts | null, visible: boolean, sourceRevision: number) {
  const previous = useRef({ sourceRevision, known: {} as Partial<Record<SessionKind, Set<string>>> });
  const automaticTimer = useRef<number>(undefined);
  const hoverTimer = useRef<number>(undefined);
  const hoverTarget = useRef<OverlayTarget | null>(null);
  const [automatic, setAutomatic] = useState(NO_KINDS);
  const [hovered, setHovered] = useState(NO_KINDS);

  useEffect(() => {
    const changedSource = previous.current.sourceRevision !== sourceRevision;
    if (changedSource) previous.current = { sourceRevision, known: {} };
    if (!visible || !sessions || changedSource) {
      window.clearTimeout(automaticTimer.current);
      window.clearTimeout(hoverTimer.current);
      hoverTarget.current = null;
      setAutomatic(NO_KINDS);
      setHovered(NO_KINDS);
    }
    if (!sessions) return;
    const updates = {
      working: sessions.workingDetails.map(session => session.id),
      completed: sessions.unreadCompletedDetails?.map(session => JSON.stringify([session.id, session.completionId])),
    };
    const additions = kinds.filter(kind => {
      const ids = updates[kind];
      if (!ids) return false;
      const baseline = previous.current.known[kind];
      previous.current.known[kind] = new Set(ids);
      return baseline !== undefined && ids.some(id => !baseline.has(id));
    });
    if (!visible || additions.length === 0) return;
    const includeAdditions = (current: SessionKind[]) => kinds.filter(kind => current.includes(kind) || additions.includes(kind));
    setAutomatic(includeAdditions);
    if (hoverTarget.current === 'details') setHovered(includeAdditions);
    window.clearTimeout(automaticTimer.current);
    automaticTimer.current = window.setTimeout(() => setAutomatic(NO_KINDS), AUTO_OPEN_MS);
  }, [sessions, visible, sourceRevision]);

  useEffect(() => () => {
    window.clearTimeout(automaticTimer.current);
    window.clearTimeout(hoverTimer.current);
  }, []);

  const groups = useMemo(() => {
    if (!visible || !sessions || previous.current.sourceRevision !== sourceRevision) return [];
    const details = { working: sessions.workingDetails, completed: sessions.unreadCompletedDetails };
    return kinds.flatMap(kind => {
      const items = details[kind];
      return items?.length && (automatic.includes(kind) || hovered.includes(kind))
        ? [{ kind, sessions: items }]
        : [];
    });
  }, [sessions, visible, sourceRevision, automatic, hovered]);

  function hover(target: OverlayTarget | null, crossesRing: boolean) {
    if (!visible) return;
    hoverTarget.current = target;
    window.clearTimeout(hoverTimer.current);
    if (target === null) {
      // 左侧展开时需要穿过圆环，留出比右侧间隙更长的时间。
      hoverTimer.current = window.setTimeout(() => setHovered(NO_KINDS), crossesRing ? 700 : 260);
    } else {
      setHovered(target === 'details' ? groups.map(group => group.kind) : [target]);
    }
  }

  return { groups, hover };
}
