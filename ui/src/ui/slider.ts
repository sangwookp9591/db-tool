// React Bits interaction port (MIT + Commons Clause), original author David Haz.
import type { MicroAccessibility } from './accessibility';
export interface SliderProps extends MicroAccessibility {
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  disabled?: boolean;
  unit?: string;
}

export function clampValue(value: number, min: number, max: number, step = 1) {
  if (!Number.isFinite(value) || max <= min) return min;
  const clamped = Math.min(max, Math.max(min, value));
  if (!(step > 0)) return clamped;
  const grid = Math.min(max, Math.max(min, min + Math.round((clamped - min) / step) * step));
  const closest = Math.abs(max - clamped) < Math.abs(grid - clamped) ? max : grid;
  return Number(closest.toFixed(8));
}

export function sliderKey(key: string, value: number, min: number, max: number, step = 1, shift = false) {
  let next: number;
  switch (key) {
    case 'Home': next = min; break;
    case 'End': next = max; break;
    case 'ArrowUp': case 'ArrowRight': next = value + step * (shift ? 10 : 1); break;
    case 'ArrowDown': case 'ArrowLeft': next = value - step * (shift ? 10 : 1); break;
    case 'PageUp': next = value + step * 10; break;
    case 'PageDown': next = value - step * 10; break;
    default: return undefined;
  }
  return clampValue(next, min, max, step);
}
