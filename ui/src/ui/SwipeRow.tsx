import { Button } from './Button';
// Based on React Bits (MIT + Commons Clause), original author David Haz.
import type { ComponentChildren } from 'preact';
import { useId, useLayoutEffect, useRef } from 'preact/hooks';
import { spring } from './spring';
import type { MicroAccessibility } from './accessibility';
import './micro.css';

export interface SwipeAction { id: string; label: string; icon?: ComponentChildren; onSelect: () => void; disabled?: boolean }
export interface SwipeRowProps extends MicroAccessibility { open: boolean; onOpenChange: (open: boolean) => void; actions: readonly SwipeAction[]; children: ComponentChildren; label: string; disabled?: boolean }
export function SwipeRow({ open, onOpenChange, actions, children, label, disabled, ...accessibility }: SwipeRowProps) {
  const id = useId();
  const root = useRef<HTMLDivElement>(null);
  const surface = useRef<HTMLDivElement>(null);
  const rail = useRef<HTMLDivElement>(null);
  const grip = useRef<{ id: number; x: number; y: number; offset: number; moved: boolean } | null>(null);
  const x = useRef(0);
  const stop = useRef<() => void>(() => {});
  const wheelEnd = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const width = () => rail.current?.offsetWidth ?? actions.length * 64;
  const paint = (next: number) => { x.current = next; if (surface.current) surface.current.style.transform = `translateX(${next}px)`; };
  const settle = () => { stop.current(); stop.current = spring(x.current, open ? -width() : 0, paint); };
  useLayoutEffect(() => { settle(); }, [open, actions.length]);
  useLayoutEffect(() => () => { stop.current(); clearTimeout(wheelEnd.current); }, []);
  useLayoutEffect(() => {
    const element = root.current!;
    const wheel = (e: WheelEvent) => {
      if (disabled || !e.deltaX || Math.abs(e.deltaX) <= Math.abs(e.deltaY)) return;
      e.preventDefault(); stop.current();
      paint(Math.min(0, Math.max(-width(), x.current - e.deltaX)));
      clearTimeout(wheelEnd.current);
      wheelEnd.current = setTimeout(() => { const next = x.current < -width() / 2; onOpenChange(next); if (next === open) settle(); }, 100);
    };
    element.addEventListener('wheel', wheel, { passive: false });
    return () => element.removeEventListener('wheel', wheel);
  }, [open, disabled, onOpenChange, actions.length]);
  const end = (cancel = false) => {
    if (!grip.current) return;
    const moved = grip.current.moved;
    grip.current = null;
    if (cancel || !moved) { settle(); return; }
    const next = x.current < -width() / 2;
    onOpenChange(next);
    if (next === open) settle();
  };
  return <div ref={root} className="swipe-row micro" role="group" aria-label={accessibility['aria-label'] ?? accessibility.ariaLabel ?? label} data-testid={accessibility['data-testid'] ?? accessibility.testId} data-open={open} aria-disabled={disabled || undefined}>
    <div ref={rail} id={id} className="swipe-row__actions" aria-hidden={!open} inert={!open}>
      {actions.map(action => <Button bare type="button" key={action.id} disabled={disabled || action.disabled} onClick={() => action.onSelect()}>{action.icon}{action.label}</Button>)}
    </div>
    <div ref={surface} className="swipe-row__surface" onPointerDown={e => {
      if (disabled || e.button !== 0 || (e.target as HTMLElement).closest('button,input,a,select,textarea')) return;
      stop.current();
      grip.current = { id: e.pointerId, x: e.clientX, y: e.clientY, offset: x.current, moved: false };
      e.currentTarget.setPointerCapture(e.pointerId);
    }} onPointerMove={e => {
      const g = grip.current;
      if (!g || g.id !== e.pointerId || disabled) return;
      const dx = e.clientX - g.x;
      if (!g.moved && (Math.abs(dx) < 8 || Math.abs(dx) < Math.abs(e.clientY - g.y))) return;
      g.moved = true;
      paint(Math.min(0, Math.max(-width(), g.offset + dx)));
    }} onPointerUp={() => end()} onPointerCancel={() => end(true)} onLostPointerCapture={() => end(true)}>
      <div className="swipe-row__body">{children}</div>
      <Button bare type="button" className="swipe-row__toggle" disabled={disabled || !actions.length} aria-label={`${label} actions`} aria-expanded={open} aria-controls={id} aria-keyshortcuts="ArrowRight"
        onClick={() => onOpenChange(!open)} onKeyDown={e => {
          if (['ArrowRight', 'ArrowLeft', 'Escape'].includes(e.key)) { e.preventDefault(); onOpenChange(e.key === 'ArrowRight'); }
        }}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m9 5 7 7-7 7" /></svg></Button>
    </div>
    <div className="swipe-row__quick" aria-label={`${label} quick actions`} aria-hidden={open} inert={open}>
      {actions.map(action => <Button bare key={action.id} type="button" title={action.label} aria-label={action.label} disabled={disabled || action.disabled} onClick={action.onSelect}>{action.icon ?? <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m9 5 7 7-7 7" /></svg>}</Button>)}
    </div>
  </div>;
}
