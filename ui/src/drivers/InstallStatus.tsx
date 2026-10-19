import { Check, ShieldCheck, Download, X, RefreshCw } from 'lucide-preact';
import { Button } from '../ui/Button';
import { cancelDriver, changeDriver, stages, useDrivers } from './api';
const order = ['download', 'verify', 'signature', 'extract', 'handshake'];
export function bytes(value: number) { return value < 1024 * 1024 ? `${Math.round(value / 1024)} KB` : `${(value / 1024 / 1024).toFixed(1)} MB`; }
export function InstallStatus({ compact = false, onRetry }: { compact?: boolean; onRetry?: (kind: 'install' | 'update' | 'remove', source: string) => void }) {
  const { busy, progress, queue, catalog, error, failed, notice } = useDrivers();
  const downloaded = progress?.bytes ?? progress?.downloaded ?? 0, total = progress?.total ?? 0, speed = progress?.bytesPerSecond ?? 0;
  const percent = total > 0 ? Math.min(100, downloaded / total * 100) : 0;
  const remaining = total > downloaded && speed > 0 ? Math.ceil((total - downloaded) / speed) : null;
  const stage = progress?.stage === 'sha256' ? 'verify' : progress?.stage ?? 'download';
  const index = ['register', 'complete'].includes(stage) ? order.length : order.indexOf(stage);
  const title = catalog.drivers.find(driver => driver.id === busy)?.name ?? busy?.split(/[/\\]/).pop();
  if (!busy && !error && !queue.length) return notice ? <p class="driver-notice" role="status">{notice}</p> : null;
  return <section class={`install-status ${compact ? 'install-status--compact' : ''}`} aria-label="설치 작업">
    {busy && <><div class="install-status-title"><span class="install-status-mark"><Download /></span><div><strong>{title}</strong><small role="status">{stages[stage] ?? '설치 준비'}{total > 0 && ` · ${Math.round(percent)}%`}</small></div><Button morph={false} disabled={['register', 'complete'].includes(stage)} aria-label="드라이버 설치 취소" onClick={() => void cancelDriver()}><X />취소</Button></div>
      <progress aria-label="드라이버 설치 진행률" value={percent} max={100} />
      <div class="install-transfer"><span>{bytes(downloaded)}{total > 0 && ` / ${bytes(total)}`}</span><span>{stage !== 'download' ? '파일 확인 중' : speed > 0 ? `${bytes(speed)}/s` : '속도 계산 중'}{stage === 'download' && remaining !== null && ` · 약 ${remaining}초 남음`}</span></div>
      {!compact && <ol class="install-steps">{order.map((value, i) => <li data-current={i === index} data-done={i < index} key={value}>{i < index ? <Check /> : <ShieldCheck />}<span>{stages[value]}</span></li>)}</ol>}</>}
    {!!queue.length && <div class="install-queue" aria-label="설치 대기열">{compact ? <small>설치 대기열 · {queue.filter(item => item.state === 'done').length}/{queue.length} 완료{queue.some(item => item.state === 'error') ? ' · 실패' : queue.some(item => item.state === 'cancelled') ? ' · 취소됨' : ''}</small> : queue.map(item => <span key={item.source} data-state={item.state}>{catalog.drivers.find(driver => driver.id === item.source)?.name ?? item.source}<small>{{ waiting: '대기', running: '설치 중', done: '완료', error: '실패', cancelled: '취소됨' }[item.state]}</small></span>)}</div>}
    {error && <div class="driver-error" role="alert"><strong>설치하지 못했습니다</strong><p>{error}</p>{failed && <Button morph={false} onClick={() => onRetry ? onRetry(failed.kind, failed.source) : void changeDriver(failed.kind, failed.source).catch(() => {})}><RefreshCw />다시 시도</Button>}</div>}
    {notice && <small role="status">{notice}</small>}
  </section>;
}
