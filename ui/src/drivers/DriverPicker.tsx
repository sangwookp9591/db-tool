import { useLayoutEffect, useState } from 'preact/hooks';
import { Button } from '../ui/Button';
import { installDrivers, refreshDrivers, useDrivers } from './api';
import { driverSize } from './DriverCard';
import { DriverGallery, DriversSkeleton } from './DriverGallery';
import { InstallStatus } from './InstallStatus';
import './drivers.css';
export function DriverPicker({ onChanged, compact = false, onBusyChange }: { onChanged?: () => void; compact?: boolean; onBusyChange?: (busy: boolean) => void }) {
  const { catalog, busy } = useDrivers();
  const [selected, setSelected] = useState<string[]>([]), [installing, setInstalling] = useState(false);
  const locked = installing || !!busy;
  useLayoutEffect(() => { onBusyChange?.(locked); }, [locked, onBusyChange]);
  async function install() {
    if (locked) return;
    setInstalling(true);
    try { await installDrivers(selected); setSelected([]); onChanged?.(); }
    finally { setInstalling(false); }
  }
  return <section class={`driver-picker ${compact ? 'driver-picker--compact' : ''}`} aria-label="DB 드라이버 설치">
    <div class="driver-picker-heading"><strong>필요한 드라이버 선택</strong><small role="status">{catalog.refreshing ? '최신 목록 확인 중…' : '여러 개 선택 가능'}</small></div>
    <div class="driver-picker-list">{catalog.drivers.length ? <DriverGallery drivers={catalog.drivers} scope="onboarding" selected={selected} disabled={locked} onSelect={driver => setSelected(ids => ids.includes(driver.id) ? ids.filter(id => id !== driver.id) : [...ids, driver.id])}>{driver => <div class="driver-glass-meta"><strong>{driver.name}</strong><small>{driver.installed_version ? '설치됨' : `${driver.version} · ${driverSize(driver.size)}`}</small></div>}</DriverGallery> : catalog.refreshing ? <DriversSkeleton compact /> : <p>설치 가능한 목록이 없습니다. <Button morph={false} onClick={() => void refreshDrivers()}>다시 확인</Button></p>}</div>
    {catalog.warning && <p class="driver-error" role="alert">{catalog.warning}</p>}
    <InstallStatus compact />
    <Button morph={false} variant="primary" disabled={locked || !selected.length} onClick={() => void install()}>{installing ? '설치 중…' : `선택한 드라이버 ${selected.length}개 설치`}</Button>
  </section>;
}
