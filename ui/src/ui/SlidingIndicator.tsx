// Interaction reference: affectUI Segmented Control by Flo; independent Preact implementation, no third-party source copied.
import type { ComponentChildren } from 'preact';
import { useLayoutEffect, useRef } from 'preact/hooks';
import type { MicroAccessibility } from './accessibility';
import { spring } from './spring';
import './micro.css';

export function SlidingIndicator({ value, children, ...accessibility }: MicroAccessibility & { value: string; children: ComponentChildren }) {
  const root = useRef<HTMLDivElement>(null);
  const line = useRef<HTMLSpanElement>(null);
  const position = useRef<{ x: number; y: number; width: number } | null>(null);
  const stop = useRef<() => void>(() => {});
  const measured = useRef(false);
  const measure = (animate = true) => {
      stop.current();
      const active = root.current?.querySelector<HTMLElement>('[aria-selected=true],[aria-checked=true]');
      if (!active || !line.current || !root.current) { if (line.current) line.current.style.opacity = '0'; position.current = null; return; }
      const next = { x: 0, y: active.offsetHeight, width: active.offsetWidth };
      for (let node: HTMLElement | null = active; node && node !== root.current; node = node.offsetParent as HTMLElement | null) { next.x += node.offsetLeft; next.y += node.offsetTop; }
      for (let node = active.parentElement; node && node !== root.current; node = node.parentElement) { next.x -= node.scrollLeft; next.y -= node.scrollTop; }
      next.width = Math.max(0, Math.min(root.current.clientWidth, next.x + next.width) - Math.max(0, next.x));
      next.x = Math.max(0, next.x);
      const from = position.current ?? next;
      line.current.style.opacity = '1';
      const paint = (t: number) => {
        const at = { x: from.x + (next.x - from.x) * t, y: from.y + (next.y - from.y) * t, width: from.width + (next.width - from.width) * t };
        position.current = at;
        if (line.current) line.current.style.transform = `translate(${at.x}px, ${at.y}px) scaleX(${at.width})`;
      };
      if (!animate || (from.x === next.x && from.y === next.y && from.width === next.width)) paint(1);
      else stop.current = spring(0, 1, paint);
    };
  useLayoutEffect(() => { if (measured.current) measure(); }, [value, children]);
  useLayoutEffect(() => {
    const reposition = () => { measured.current = true; measure(false); };
    const observer = new ResizeObserver(reposition);
    observer.observe(root.current!);
    for (const child of root.current!.querySelectorAll('[role=tab],[role=radio]')) observer.observe(child);
    root.current!.addEventListener('scroll', reposition, true);
    const element = root.current!;
    return () => { stop.current(); observer.disconnect(); element.removeEventListener('scroll', reposition, true); };
  }, []);
  const label = accessibility['aria-label'] ?? accessibility.ariaLabel;
  return <div ref={root} className="sliding-indicator micro" role={label ? 'group' : undefined} aria-label={label} data-testid={accessibility['data-testid'] ?? accessibility.testId}>
    {children}<span ref={line} className="sliding-indicator__line" aria-hidden="true" />
  </div>;
}
