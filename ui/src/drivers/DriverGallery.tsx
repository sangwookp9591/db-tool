import { useDrivers, type CatalogDriver } from './api';
import { DriverTile } from './DriverTile';

export function DriversSkeleton({ compact = false }: { compact?: boolean }) {
  return <div class={`driver-gallery drivers-skeleton ${compact ? 'driver-gallery--picker' : ''}`} role="status" aria-label="드라이버 목록 준비 중">{Array.from({ length: 6 }, (_, i) => <div class="driver-skeleton-cell" key={i}><span class="driver-skeleton-icon" /><span class="driver-skeleton-line" /></div>)}</div>;
}

export function DriverGallery({ drivers, scope, selected, disabled, onSelect, onAction, onAddConnection }: {
  drivers: CatalogDriver[]; scope: string; selected?: string[]; disabled?: boolean;
  onSelect?: (driver: CatalogDriver) => void;
  onAction?: (kind: 'install' | 'update' | 'remove', id: string) => void;
  onAddConnection?: () => void;
}) {
  const state = useDrivers();
  function navigate(event: KeyboardEvent & { currentTarget: HTMLDivElement }) {
    if (!['ArrowRight', 'ArrowLeft', 'ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key) || !(event.target as HTMLElement).matches('.driver-tile')) return;
    const buttons = [...event.currentTarget.querySelectorAll<HTMLButtonElement>('.driver-tile')];
    const index = buttons.indexOf(event.target as HTMLButtonElement);
    const columns = getComputedStyle(event.currentTarget).gridTemplateColumns.split(' ').length;
    const step = ({ ArrowRight: 1, ArrowLeft: -1, ArrowDown: columns, ArrowUp: -columns, Home: 1, End: -1 } as Record<string, number>)[event.key];
    let next = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : index + step;
    while (buttons[next]?.disabled) next += step;
    event.preventDefault();
    event.stopPropagation();
    buttons[next]?.focus();
  }
  return <div class={`driver-gallery ${scope === 'onboarding' ? 'driver-gallery--picker' : ''}`} onKeyDown={navigate}>
    {drivers.map(driver => <DriverTile key={driver.id} driver={driver} state={state} selected={selected ? !!driver.installed_version || selected.includes(driver.id) : undefined} disabled={!!selected && (disabled || !!driver.installed_version)} onSelect={onSelect} onAction={onAction} onAddConnection={onAddConnection} />)}
  </div>;
}
