// Based on React Bits (MIT + Commons Clause), original author David Haz.
import type { ComponentChildren } from 'preact';
import { useLayoutEffect, useRef } from 'preact/hooks';
import { spring } from './spring';
import type { MicroAccessibility } from './accessibility';
import './micro.css';

export interface SpringCheckProps extends MicroAccessibility { checked: boolean; onChange: (checked: boolean) => void; label: ComponentChildren; disabled?: boolean }
export function SpringCheck({ checked, onChange, label, disabled, ...accessibility }: SpringCheckProps) {
  const fill = useRef<HTMLSpanElement>(null);
  const tick = useRef<SVGSVGElement>(null);
  const current = useRef(checked ? 1 : 0);
  useLayoutEffect(() => spring(current.current, checked ? 1 : 0, value => {
    current.current = value;
    if (fill.current) fill.current.style.transform = `scale(${value})`;
    if (tick.current) { tick.current.style.transform = `scale(${value})`; tick.current.style.opacity = String(value); }
  }), [checked]);
  return <label className="spring-check micro">
    <input type="checkbox" checked={checked} disabled={disabled} aria-label={accessibility['aria-label'] ?? accessibility.ariaLabel} data-testid={accessibility['data-testid'] ?? accessibility.testId} onChange={e => onChange(e.currentTarget.checked)} />
    <span className="spring-check__box" aria-hidden="true"><span ref={fill} className="spring-check__fill" style={{ transform: `scale(${checked ? 1 : 0})` }} />
      <svg ref={tick} viewBox="0 0 24 24" style={{ transform: `scale(${checked ? 1 : 0})`, opacity: checked ? 1 : 0 }}><path d="m5 12 4 4 10-10" /></svg>
    </span><span>{label}</span>
  </label>;
}
