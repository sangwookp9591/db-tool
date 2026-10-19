import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEffect, useRef, useState } from 'preact/hooks';
import { activeRequests, freezeRequests, isMock } from './api';

export interface QueryDraft { params: string; rowLimit: number; timeout: number; paneRatio: number }
export const restoreWorkspace = <T>() => isMock ? Promise.resolve(null) : invoke<T | null>('recycle_restore');
export function noteLargeResult(rows: number) {
  if (!isMock && rows >= 10000) void invoke('recycle_large').catch(() => {});
}
export function useIdleRecycle(snapshot: () => unknown | null, ready: boolean, report: (error: unknown) => void) {
  const latest = useRef(snapshot); latest.current = snapshot;
  const [enabled, setEnabled] = useState(false);
  useEffect(() => {
    if (!isMock) void invoke<boolean>('recycle_settings', { value: null }).then(setEnabled).catch(report);
  }, []);
  useEffect(() => {
    if (isMock || !ready) return;
    let alive = true;
    const subscription = listen<number>('recycle-prepare', async event => {
      if (!alive) return;
      const value = activeRequests === 0 ? latest.current() : null;
      freezeRequests(true);
      const wasInert = document.documentElement.inert;
      document.documentElement.inert = true;
      try { await invoke('recycle_commit', { generation: event.payload, snapshot: value }); }
      catch (error) { report(error); }
      finally { freezeRequests(false); document.documentElement.inert = wasInert; }
    });
    void subscription.then(() => invoke('recycle_ready')).catch(report);
    return () => { alive = false; void subscription.then(unlisten => unlisten()); };
  }, [ready]);
  async function configure(value: boolean) {
    if (isMock) { setEnabled(value); return; }
    try { setEnabled(await invoke<boolean>('recycle_settings', { value })); }
    catch (error) { report(error); }
  }
  return { enabled, configure };
}
