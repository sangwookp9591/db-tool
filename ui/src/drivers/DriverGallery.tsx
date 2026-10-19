import { lazy, Suspense } from 'preact/compat';
import { useEffect, useRef, useState } from 'preact/hooks';
import type { ComponentChildren } from 'preact';
import { Check } from 'lucide-preact';
import type { CatalogDriver } from './api';
import { driverBrand } from './brands';
const GlassIcons = lazy(() => import('../ui/GlassIcons'));
const entered = new Set<string>();
export function DriversSkeleton({ compact = false }: { compact?: boolean }) {
  return <div class={`drivers-skeleton ${compact ? 'drivers-skeleton--compact' : ''}`} role="status" aria-label="드라이버 목록 준비 중">{Array.from({ length: 5 }, (_, i) => <div class="driver-skeleton-cell" key={i}><span class="driver-skeleton-icon" /><span class="driver-skeleton-line" /><span class="driver-skeleton-line short" /></div>)}</div>;
}
export function DriverGallery({ drivers, scope, selected, disabled, onSelect, children }: { drivers: CatalogDriver[]; scope: string; selected?: string[]; disabled?: boolean; onSelect: (driver: CatalogDriver) => void; children: (driver: CatalogDriver) => ComponentChildren }) {
  const root = useRef<HTMLDivElement>(null);
  const intro = useRef(!entered.has(scope));
  const [theme, setTheme] = useState(document.documentElement.dataset.theme === 'daylight' ? 'light' : 'dark');
  const [columns, setColumns] = useState(3);
  useEffect(() => {
    entered.add(scope);
    const sync = () => setTheme(document.documentElement.dataset.theme === 'daylight' ? 'light' : 'dark');
    const element = root.current!;
    const observer = new ResizeObserver(() => setColumns(element.clientWidth < 420 ? 2 : 3));
    observer.observe(element); window.addEventListener('axdb-theme-change', sync);
    return () => { observer.disconnect(); window.removeEventListener('axdb-theme-change', sync); };
  }, []);
  return <div class="driver-gallery" ref={root}><Suspense fallback={<DriversSkeleton compact={scope === 'onboarding'} />}><GlassIcons columns={columns} size={78} gap={66} roundness={0.62} refraction={1.35} bevel={1} dispersion={0} frost={0.2} shine={1} tint="#dfdfdf" tintOpacity={0.2} spread={0} parallax={1} plate="offset" hover="lift" labels="hover" iconColor="#ffffff" intro={intro.current} theme={theme as 'light' | 'dark'} items={drivers.map(driver => ({ id: driver.id, ...driverBrand(driver.id), label: driver.name, ariaLabel: selected ? `${driver.name} 선택` : `${driver.name} 상세`, testId: `driver-${driver.id}`, checkbox: !!selected, selected: selected ? !!driver.installed_version || selected.includes(driver.id) : undefined, disabled: !!selected && (disabled || !!driver.installed_version), onClick: () => onSelect(driver), badge: driver.installed_version ? <><Check />설치됨</> : selected?.includes(driver.id) ? <Check /> : undefined, content: children(driver) }))} /></Suspense></div>;
}
