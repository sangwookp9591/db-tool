import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';
import { Download, Plus, RefreshCw, X } from 'lucide-preact';
import { Button } from '../ui/Button';
import { Tooltip } from '../ui/Tooltip';
import { StatusMark } from '../ui/StatusMark';
import { FuseButton } from '../ui/FuseButton';
import { cancelDriver, stages, type CatalogDriver, type useDrivers } from './api';
import { driverSize } from './DriverCard';
import { driverBrand } from './brands';
import { DriverLogo } from './DriverLogo';

type Props = {
  driver: CatalogDriver; state: ReturnType<typeof useDrivers>; selected?: boolean; disabled?: boolean;
  onSelect?: (driver: CatalogDriver) => void;
  onAction?: (kind: 'install' | 'update' | 'remove', id: string) => void;
  onAddConnection?: () => void;
};
export function DriverTile({ driver, state, selected, disabled, onSelect, onAction, onAddConnection }: Props) {
  const [tip, setTip] = useState(false), [open, setOpen] = useState(false), [pending, setPending] = useState(false);
  const button = useRef<HTMLButtonElement>(null), popup = useRef<HTMLDivElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined), touchTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const held = useRef(false), touch = useRef(false);
  const running = state.busy === driver.id, installed = !!driver.installed_version;
  const failed = state.failed?.source === driver.id && !!state.error;
  const update = installed && driver.installed_version !== driver.version;
  const percent = state.progress?.total ? Math.min(100, Math.round((state.progress.bytes ?? state.progress.downloaded) / state.progress.total * 100)) : 0;
  const status = running ? `${stages[state.progress?.stage ?? ''] ?? '설치 준비'} ${percent}%` : failed ? '설치 실패 · 다시 시도' : update ? '설치됨 · 업데이트 가능' : installed ? '설치됨' : selected ? '선택됨' : '설치 가능';
  const brand = driverBrand(driver.icon_key);
  const showTip = useCallback((show: boolean) => {
    clearTimeout(timer.current);
    if (!show) setTip(false);
    else if (!touch.current) timer.current = setTimeout(() => setTip(true), 300);
  }, []);
  useEffect(() => () => { clearTimeout(timer.current); clearTimeout(touchTimer.current); }, []);
  useLayoutEffect(() => {
    if (!open || !popup.current || !button.current) return;
    const element = popup.current;
    element.showPopover();
    const rect = button.current.getBoundingClientRect();
    element.style.left = `${Math.max(8, Math.min(rect.left, innerWidth - element.offsetWidth - 8))}px`;
    element.style.top = `${rect.bottom + element.offsetHeight + 8 < innerHeight ? rect.bottom + 4 : Math.max(8, rect.top - element.offsetHeight - 4)}px`;
    element.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
    return () => { if (element.matches(':popover-open')) element.hidePopover(); };
  }, [open]);
  function close() { setOpen(false); setPending(false); button.current?.focus(); }
  function act(kind: 'install' | 'update' | 'remove') { close(); onAction?.(kind, driver.id); }
  function activate() {
    if (held.current) { held.current = false; return; }
    clearTimeout(timer.current); setTip(false);
    if (selected !== undefined) { onSelect?.(driver); return; }
    if (running || installed || failed || driver.size >= 100 * 1024 * 1024) { setOpen(true); return; }
    if (!state.busy) onAction?.('install', driver.id);
  }
  return <div class="driver-tile-cell" data-testid={`driver-${driver.id}`} data-state={running ? 'running' : failed ? 'failed' : update ? 'update' : installed ? 'installed' : 'available'}>
    <Tooltip open={tip && !open} onOpenChange={showTip} content={<span class="driver-tip"><strong>{driver.name}</strong><small>{driver.installed_version ?? driver.version} · {driverSize(driver.size)} · {driver.paradigm}</small><span>{driver.description}</span><small>{status}</small>{failed && <span class="driver-error">{state.error} · 클릭하여 다시 시도</span>}</span>}>
      <button ref={button} type="button" class="driver-tile" role={selected !== undefined ? 'checkbox' : undefined} aria-checked={selected} disabled={disabled} aria-disabled={selected === undefined && !!state.busy && !running} aria-label={selected !== undefined ? `${driver.name} 선택` : `${driver.name} · ${status}`} aria-description={selected !== undefined ? status : undefined} aria-haspopup={selected === undefined && (installed || running || failed || driver.size >= 100 * 1024 * 1024) ? 'dialog' : undefined} aria-expanded={open || undefined} onClick={activate}
        onFocus={event => { if (event.currentTarget.matches(':focus-visible')) { touch.current = false; showTip(true); } }}
        onKeyDown={event => { if (event.key !== 'Escape') { touch.current = false; showTip(true); } }}
        onPointerEnter={event => { touch.current = event.pointerType === 'touch'; }}
        onPointerDown={event => { if (event.pointerType !== 'touch') return; touch.current = true; held.current = false; clearTimeout(timer.current); touchTimer.current = setTimeout(() => { held.current = true; setTip(true); }, 500); }}
        onPointerUp={() => clearTimeout(touchTimer.current)} onPointerCancel={() => { clearTimeout(touchTimer.current); held.current = true; }} onPointerLeave={() => clearTimeout(touchTimer.current)}>
        <span class="driver-tile-icon" style={{ '--driver-brand': brand?.color ?? 'var(--border-default)', '--driver-accent': brand?.accent ?? brand?.color ?? 'var(--accent-primary)' }}>
          <DriverLogo iconKey={driver.icon_key} paradigm={driver.paradigm} size={36} />
          {running && <svg class="driver-tile-ring" viewBox="0 0 60 60" aria-hidden="true"><rect x="2" y="2" width="56" height="56" rx="15" pathLength="100" stroke-dasharray={`${percent} 100`} /></svg>}
          <span class="driver-tile-badge" data-position={installed && !running && !failed ? 'bottom' : 'top'} aria-hidden="true">
            <StatusMark status={running ? 'running' : failed ? 'failed' : installed || selected ? 'done' : 'pending'} progress={running ? percent / 100 : undefined} size={16} strike={false} />
            {!running && !failed && !installed && !selected && <Plus class="driver-tile-plus" />}
            {failed && <span class="driver-tile-failed">!</span>}
            {running && <small>{percent}%</small>}
          </span>
          {update && !running && <span class="driver-update-dot" aria-hidden="true" />}
        </span>
        <span class="driver-tile-name">{driver.name}</span>
      </button>
    </Tooltip>
    {open && <div ref={popup} popover="auto" role="dialog" aria-label={`${driver.name} 관리`} class="driver-popover" onToggle={event => { if (event.newState === 'closed') { setOpen(false); setPending(false); } }}>
      <div class="driver-popover-title"><strong>{driver.name}</strong><Button morph={false} variant="ghost" aria-label="드라이버 메뉴 닫기" onClick={close}><X /></Button></div>
      <small>{driver.installed_version ?? driver.version} · {driverSize(driver.size)}</small>
      {running ? <><p>{status}</p><Button morph={false} disabled={['register', 'complete'].includes(state.progress?.stage ?? '')} onClick={() => { close(); void cancelDriver(); }}>설치 취소</Button></> : failed ? <><p class="driver-error">{state.error}</p><Button morph={false} disabled={!!state.busy} onClick={() => act(state.failed?.kind ?? 'install')}><RefreshCw />다시 시도</Button></> : installed ? <>
        <Button morph={false} disabled={!!state.busy || pending} onClick={() => { close(); onAddConnection?.(); }}><Plus />연결 추가</Button>
        {update && <Button morph={false} disabled={!!state.busy || pending} onClick={() => act('update')}><RefreshCw />{driver.version} 업데이트</Button>}
        <FuseButton pending={pending} onPendingChange={setPending} onCommit={() => act('remove')} onUndo={() => {}} unmountAction="cancel" disabled={!!state.busy} aria-label={`${driver.name} 삭제`} label="삭제" doneLabel="5초 후 삭제" undoLabel="되돌리기" />
      </> : <><p>이 패키지는 {driverSize(driver.size)}를 다운로드합니다.</p><Button morph={false} disabled={!!state.busy} onClick={() => act('install')}><Download />설치</Button></>}
    </div>}
  </div>;
}
