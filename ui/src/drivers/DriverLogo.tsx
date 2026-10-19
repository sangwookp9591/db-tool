import { Database, FileJson, KeyRound, Share2 } from 'lucide-preact';
import type { DriverManifest } from '../contract';
import { driverBrand } from './brands';
import './driver-logo.css';
export function DriverLogo({ iconKey, paradigm, size = 18 }: { iconKey?: string; paradigm?: DriverManifest['paradigm']; size?: number }) {
  const brand = driverBrand(iconKey);
  const Icon = paradigm === 'Graph' ? Share2 : paradigm === 'Document' ? FileJson : paradigm === 'KeyValue' ? KeyRound : Database;
  return <span class="driver-logo" data-brand={brand ? '' : undefined} style={{ width: size, height: size }} aria-hidden="true">{brand ? <img src={brand.image} alt="" width={size} height={size} decoding="async" draggable={false} /> : <Icon width={size} height={size} />}</span>;
}
