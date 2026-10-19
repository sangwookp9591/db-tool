import { Channel, invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'preact/hooks';
import { api, isMock } from '../api';
import type { DriverManifest } from '../contract';

export interface CatalogDriver {
  id: string; icon_key?: string; name: string; version: string; description: string;
  paradigm: DriverManifest['paradigm']; size: number; installed_version?: string | null;
  homepage?: string; license?: string; platforms?: string[]; url?: string; sha256?: string;
}
export interface Catalog { installed?: { id: string; version: string }[]; drivers: CatalogDriver[]; refreshing: boolean; warning?: string | null }
export interface InstallProgress { stage: string; downloaded: number; total?: number | null; bytes?: number; bytesPerSecond?: number }
export interface DriverCommands {
  drivers_catalog: { args: Record<string, never>; result: Catalog };
  driver_install: { args: { source: string; allowUnverified: boolean; onProgress: Pick<Channel<InstallProgress>, 'onmessage'> }; result: { id: string; version: string } };
  driver_update: { args: { id: string; onProgress: Pick<Channel<InstallProgress>, 'onmessage'> }; result: { id: string; version: string } };
  driver_remove: { args: { id: string }; result: void };
  driver_cancel: { args: Record<string, never>; result: boolean };
}
export async function driverApi<K extends keyof DriverCommands>(command: K, args: DriverCommands[K]['args']): Promise<DriverCommands[K]['result']> {
  if (isMock) return (await import('../mock')).mockDriverInvoke(command, args);
  return invoke(command, args);
}
const CACHE_KEY = `axdb-driver-catalog-v1-${isMock ? 'mock' : 'native'}`;
function cachedCatalog(): Catalog {
  try {
    const drivers: unknown = JSON.parse(localStorage.getItem(CACHE_KEY) ?? '[]');
    if (Array.isArray(drivers) && drivers.length > 0 && drivers.every(d => d && typeof d.id === 'string' && /^[a-z0-9][a-z0-9_-]{0,63}$/.test(d.id) && typeof d.name === 'string' && typeof d.version === 'string' && typeof d.description === 'string' && typeof d.size === 'number' && Number.isFinite(d.size) && d.size >= 0 && (d.installed_version == null || typeof d.installed_version === 'string') && ['Relational', 'Graph', 'Document', 'KeyValue'].includes(d.paradigm))) return { drivers, refreshing: false };
  } catch { /* A bad or unavailable cache never prevents host refresh. */ }
  return { drivers: [], refreshing: true };
}
export type QueueItem = { source: string; state: 'waiting' | 'running' | 'done' | 'error' | 'cancelled' };
type Action = { kind: 'install' | 'update' | 'remove'; source: string };
const listeners = new Set<() => void>();
let state: { catalog: Catalog; busy: string | null; progress: InstallProgress | null; error: string; notice: string; queue: QueueItem[]; failed: Action | null } = {
  catalog: cachedCatalog(), busy: null, progress: null, error: '', notice: '', queue: [], failed: null,
};
let refresh: Promise<void> | undefined;
let subscribed: Promise<unknown> | undefined;
let stopQueue = false;
let queueRunning = false;
function publish(patch: Partial<typeof state>) { state = { ...state, ...patch }; listeners.forEach(listener => listener()); }
export function driverError(error: unknown) { return error instanceof Error ? error.message : typeof error === 'object' && error && 'message' in error ? String(error.message) : String(error); }
async function acceptCatalog(catalog: Catalog) {
  publish({ catalog });
  try { localStorage.setItem(CACHE_KEY, JSON.stringify(catalog.drivers)); } catch { /* In-memory cache is still available. */ }
  const missing = catalog.installed?.filter(item => !catalog.drivers.some(driver => driver.id === item.id)) ?? [];
  if (!missing.length) return;
  try {
    const manifests = await api('drivers_list', {});
    if (state.catalog !== catalog) return;
    publish({ catalog: { ...catalog, drivers: [...catalog.drivers, ...missing.map(item => {
      const manifest = manifests.find(driver => driver.id === item.id);
      return { id: item.id, name: manifest?.display_name ?? item.id, icon_key: manifest?.icon_key, version: item.version, installed_version: item.version, size: 0, description: '파일에서 설치한 드라이버입니다.', paradigm: manifest?.paradigm ?? 'Relational' as const };
    })] } });
  } catch (error) { publish({ error: driverError(error) }); }
}
export async function refreshDrivers() {
  if (refresh) return refresh;
  refresh = (async () => {
    try {
      if (!isMock) await (subscribed ??= listen<Catalog>('drivers-catalog-updated', event => { void acceptCatalog(event.payload); }).catch(error => { subscribed = undefined; throw error; }));
      publish({ catalog: { ...state.catalog, refreshing: true }, error: '' });
      await acceptCatalog(await driverApi('drivers_catalog', {}));
    } catch (error) { publish({ catalog: { ...state.catalog, refreshing: false, warning: driverError(error) } }); }
    finally { refresh = undefined; }
  })();
  return refresh;
}
export async function changeDriver(kind: Action['kind'], source: string, allowUnverified = false) {
  if (state.busy) throw new Error('다른 드라이버 작업이 끝난 뒤 다시 시도하세요.');
  if (!queueRunning) publish({ queue: [] });
  publish({ busy: source, progress: null, error: '', notice: '', failed: null });
  let frame = 0;
  let completed = false;
  let latest: InstallProgress | null = null;
  try {
    if (kind === 'remove') await driverApi('driver_remove', { id: source });
    else {
      const receive = (progress: InstallProgress) => {
        if (completed) return;
        latest = progress.stage === 'download' ? progress : { ...progress, bytes: progress.bytes || progress.downloaded || latest?.bytes || latest?.downloaded || 0, total: progress.total ?? latest?.total, bytesPerSecond: 0 };
        if (!frame) frame = requestAnimationFrame(() => { frame = 0; publish({ progress: latest }); });
      };
      const onProgress = isMock ? { onmessage: receive } : new Channel<InstallProgress>(receive);
      if (kind === 'update') await driverApi('driver_update', { id: source, onProgress });
      else await driverApi('driver_install', { source, allowUnverified, onProgress });
    }
    publish({ notice: kind === 'remove' ? '드라이버를 삭제했습니다. 연결 프로필은 유지됩니다.' : '설치가 완료되었습니다. 이제 연결을 추가할 수 있습니다.' });
    window.dispatchEvent(new Event('axdb-drivers-changed'));
    await refreshDrivers();
  } catch (error) {
    const message = driverError(error);
    if (/cancelled/i.test(message)) publish({ notice: '설치를 취소했습니다.', error: '' });
    else if (!message.includes('explicit consent required')) publish({ error: message, failed: { kind, source } });
    throw error;
  } finally { completed = true; cancelAnimationFrame(frame); publish({ busy: null, progress: null }); }
}
export async function installDrivers(sources: string[]) {
  if (queueRunning || state.busy) return;
  queueRunning = true; stopQueue = false;
  publish({ queue: sources.map(source => ({ source, state: 'waiting' })) });
  const mark = (source: string, status: QueueItem['state']) => publish({ queue: state.queue.map(item => item.source === source ? { ...item, state: status } : item) });
  try {
    for (const source of sources) {
      if (stopQueue) { mark(source, 'cancelled'); continue; }
      mark(source, 'running');
      try { await changeDriver('install', source); mark(source, 'done'); }
      catch (error) { mark(source, /cancelled/i.test(driverError(error)) ? 'cancelled' : 'error'); stopQueue = true; }
    }
    if (sources.length && state.queue.every(item => item.state === 'done')) publish({ notice: '선택한 드라이버를 설치했습니다.' });
  } finally { queueRunning = false; }
}
export async function cancelDriver() {
  stopQueue = true;
  try {
    const cancelled = await driverApi('driver_cancel', {});
    publish({ notice: cancelled ? '안전하게 취소하는 중…' : '설치 마무리 중입니다. 완료까지 잠시 기다려 주세요.' });
  } catch (error) { publish({ error: driverError(error) }); }
}
export function useDrivers() {
  const [snapshot, setSnapshot] = useState(state);
  useEffect(() => { const update = () => setSnapshot(state); listeners.add(update); void refreshDrivers(); return () => { listeners.delete(update); }; }, []);
  return snapshot;
}
export const stages: Record<string, string> = { download: '다운로드', verify: '검증', sha256: '검증', signature: '서명 확인', extract: '설치', handshake: '연결 확인', register: '등록', complete: '완료' };
