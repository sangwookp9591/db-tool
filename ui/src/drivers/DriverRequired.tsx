import { Download } from 'lucide-preact';
import { Button } from '../ui/Button';
export function openDrivers(id?: string) { window.dispatchEvent(new CustomEvent('axdb-open-drivers', { detail: id })); }
export function DriverRequired({ driverId }: { driverId?: string }) {
  return <Button morph={false} class="driver-required" onClick={() => openDrivers(driverId)}><Download />{driverId ? '드라이버 필요 · 설치' : '드라이버 설치'}</Button>;
}
