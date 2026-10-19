import { useState } from 'preact/hooks';
import { Download, RefreshCw } from 'lucide-preact';
import { Button } from '../ui/Button';
import { FuseButton } from '../ui/FuseButton';
import { stages, type CatalogDriver, type InstallProgress } from './api';

export function driverSize(bytes: number) { return bytes > 0 ? `${(bytes / 1024 / 1024).toLocaleString('ko-KR', { maximumFractionDigits: 1 })} MB` : '크기 정보 없음'; }
export function DriverProgress({ progress }: { progress: InstallProgress | null }) {
  const percent = progress?.total ? Math.min(100, Math.round(progress.downloaded / progress.total * 100)) : undefined;
  return <div class="driver-progress" role="status"><span>{stages[progress?.stage ?? ''] ?? '설치 준비'}{percent !== undefined && ` · ${percent}%`}</span><progress aria-label="드라이버 설치 진행률" max={100} value={percent ?? 0} /></div>;
}
export function DriverCard({ driver, busy, progress, onAction }: { driver: CatalogDriver; busy: string | null; progress: InstallProgress | null; onAction: (kind: 'install' | 'update' | 'remove', id: string) => void }) {
  const [pending, setPending] = useState(false);
  const installed = !!driver.installed_version;
  return <article class="driver-card" data-paradigm={driver.paradigm}>
    <div class="driver-card-heading"><div><h3>{driver.name}</h3><small>{installed ? driver.installed_version : driver.version} · {driverSize(driver.size)}</small></div></div>
    <p>{driver.description || '이 드라이버로 데이터베이스에 연결합니다.'}</p>
    {busy === driver.id ? <small role="status">{stages[progress?.stage ?? ''] ?? '설치 준비'}…</small> : <div class="driver-card-actions">{installed ? <><FuseButton pending={pending} onPendingChange={setPending} onCommit={() => onAction('remove', driver.id)} onUndo={() => {}} unmountAction="cancel" disabled={!!busy} aria-label={`${driver.name} 삭제`} label="삭제" doneLabel="5초 후 삭제" undoLabel="되돌리기" />{driver.installed_version !== driver.version && <Button morph={false} disabled={!!busy || pending} onClick={() => onAction('update', driver.id)} aria-label={`${driver.name} 업데이트`}><RefreshCw />{driver.version} 업데이트</Button>}</> : <Button morph={false} variant="primary" disabled={!!busy} onClick={() => onAction('install', driver.id)} aria-label={`${driver.name} 설치`}><Download />설치</Button>}</div>}
  </article>;
}
