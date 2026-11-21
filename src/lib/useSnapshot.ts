import { useEffect, useState } from 'react';
import { getSnapshot, initialSnapshot, subscribeState } from './api';
import { errorMessage } from './format';

export function useSnapshot() {
  const [snapshot, setSnapshot] = useState(initialSnapshot);
  useEffect(() => {
    let active = true;
    let unsubscribe: (() => void) | undefined;
    let eventReceived = false;
    async function connect() {
      try {
        const stop = await subscribeState(value => {
          eventReceived = true;
          if (active) setSnapshot(value);
        });
        if (!active) { stop(); return; }
        unsubscribe = stop;
        const value = await getSnapshot();
        if (active && !eventReceived) setSnapshot(value);
      } catch (error) {
        if (active) setSnapshot(current => ({ ...current, status: 'offline', error: errorMessage(error) }));
      }
    }
    void connect();
    return () => { active = false; unsubscribe?.(); };
  }, []);
  return snapshot;
}
