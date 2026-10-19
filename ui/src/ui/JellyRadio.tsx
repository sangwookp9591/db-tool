import { Button } from './Button';
// Based on React Bits (MIT + Commons Clause), original author David Haz.
import type { ComponentChildren } from 'preact';
import { useLayoutEffect, useRef } from 'preact/hooks';
import { spring } from './spring';
import type { MicroAccessibility } from './accessibility';
import './micro.css';

export interface JellyRadioItem extends MicroAccessibility { value: string; label: ComponentChildren; disabled?: boolean }
export interface JellyRadioProps extends MicroAccessibility {
  items: readonly (string | JellyRadioItem)[];
  value: string;
  onChange: (value: string) => void;
  role?: 'radiogroup' | 'tablist';
  disabled?: boolean;
  segmented?: boolean;
}

export function radioKey(key: string, index: number, items: readonly { disabled?: boolean }[]) {
  const dir = ['ArrowRight', 'ArrowDown', 'Home'].includes(key) ? 1 : -1;
  if (!['ArrowRight', 'ArrowDown', 'ArrowLeft', 'ArrowUp', 'Home', 'End'].includes(key)) return undefined;
  let next = key === 'Home' ? -1 : key === 'End' ? items.length : index;
  for (let i = 0; i < items.length; i++) {
    next = (next + dir + items.length) % items.length;
    if (!items[next].disabled) return next;
  }
  return undefined;
}

export function JellyRadio({ items, value, onChange, disabled, segmented = false, role = 'radiogroup', ...accessibility }: JellyRadioProps) {
  const list: JellyRadioItem[] = items.map(item => typeof item === 'string' ? { value: item, label: item } : item);
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);
  const previous = useRef(value);
  const at = list.findIndex(item => item.value === value);
  const tabbable = at >= 0 && !list[at].disabled ? at : list.findIndex(item => !item.disabled);
  useLayoutEffect(() => {
    if (segmented || previous.current === value) return;
    previous.current = value;
    return spring(0, 1, t => buttons.current.forEach((button, i) => {
      if (!button) return;
      const pulse = Math.sin(t * Math.PI);
      button.style.transform = `translateX(${Math.sign(i - at) * 4 * pulse}px) scale(${1 + (i === at ? .12 : -.025) * pulse})`;
    }));
  }, [value, at, segmented]);
  return <div className={`jelly-radio micro${segmented ? ' jelly-radio--segmented' : ''}`} style={segmented ? { '--segments': list.length, '--selected': Math.max(0, at) } : undefined} role={role} aria-label={accessibility['aria-label'] ?? accessibility.ariaLabel ?? 'Options'} data-testid={accessibility['data-testid'] ?? accessibility.testId} aria-disabled={disabled || undefined}>
    {list.map((item, i) => <Button bare key={item.value} ref={el => { buttons.current[i] = el; }} type="button" role={role === 'tablist' ? 'tab' : 'radio'}
      aria-label={item['aria-label'] ?? item.ariaLabel} data-testid={item['data-testid'] ?? item.testId}
      aria-checked={role === 'radiogroup' ? item.value === value : undefined} aria-selected={role === 'tablist' ? item.value === value : undefined} tabIndex={i === tabbable ? 0 : -1} disabled={disabled || item.disabled}
      onClick={() => onChange(item.value)} onKeyDown={e => {
        const next = radioKey(e.key, i, list);
        if (next === undefined) return;
        e.preventDefault();
        onChange(list[next].value);
        buttons.current[next]?.focus();
      }}>{item.label}</Button>)}
  </div>;
}
